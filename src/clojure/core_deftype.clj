;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(in-ns 'clojure.core)

;;;;;;;;;;;;;;;;;;;;;;; protocols ;;;;;;;;;;;;;;;;;;;;;;;;
;;
;; Adapted from JVM core_deftype.clj 508-919.
;;
;; Big-picture differences from JVM:
;;   - No Java interface (:on-interface) is generated. JVM uses interface
;;     dispatch as a fast path; we fall back entirely to the :impls map
;;     plus a per-method type→impl cache.
;;   - No clojure.lang.MethodImplCache class — just a Python dict per
;;     method, stored in the method var's meta. -reset-methods clears
;;     all caches when a protocol's :impls changes.
;;   - Type hierarchy walk uses cls.__mro__ for concrete inheritance
;;     plus an isa?-based scan over registered impls for virtual bases
;;     (Python ABCs like numbers.Number). Mirrors JVM super-chain + pref.
;;   - Protocol redefinition is allowed (alter-var-root) and resets
;;     :impls; deftype/defrecord/reify and the rest of core_deftype come
;;     in a follow-up batch.
;;
;; Skipped from JVM (need infra we don't have yet):
;;   namespace-munge, definterface, reify, hash-combine, munge,
;;   imap-cons, emit-defrecord, build-positional-factory,
;;   validate-fields, defrecord, record?, emit-deftype*, deftype.

;;; --- protocol map shape --------------------------------------------------
;;
;; The protocol var holds a map:
;;   {:name        the unqualified symbol naming the protocol
;;    :var         a pointer to the protocol's own var
;;    :doc         optional docstring
;;    :sigs        {method-kw {:name 'mname :arglists '([this x] ...) :doc d}}
;;    :method-map  {method-kw #'method-dispatch-fn}
;;    :impls       {AClass {method-kw impl-fn ...} ...}
;;    :extend-via-metadata  bool}

(defn- protocol? [maybe-p]
  (and (map? maybe-p) (boolean (:method-map maybe-p))))

(defn- find-impl-for-class
  "Returns [matched-class impl-map] or nil for protocol dispatch.

  Walks impls in priority order matching JVM (super-chain + pref):
    1. Direct hit on cls (covers cls=nil for (extend-type nil ...)).
    2. Walk cls.__mro__ excluding the universal `object` root.
       Concrete inheritance.
    3. Virtual bases: scan registered impl classes via isa?,
       excluding `object`. Picks any matching ABC.
    4. Final fallback: `object` if extended.

  Excluding `object` from steps 2-3 ensures a more specific match
  (like IKVReduce on a PersistentVector) wins over the universal
  Object fallback."
  [proto cls]
  (let [impls (:impls proto)
        obj-cls py.__builtins__/object]
    (or
      ;; 1. Direct hit.
      (when-let [m (get impls cls)] [cls m])
      (when cls
        (or
          ;; 2. Concrete inheritance via __mro__ (excluding `object`).
          (some (fn [c]
                  (when (and (not (identical? c obj-cls))
                             (contains? impls c))
                    [c (get impls c)]))
                (seq (.-__mro__ cls)))
          ;; 3. Virtual bases via isa? (excluding `object`).
          (some (fn [pair]
                  (let [ext-cls (first pair)
                        m (second pair)]
                    (when (and (class? ext-cls)
                               (not (identical? ext-cls obj-cls))
                               (isa? cls ext-cls))
                      [ext-cls m])))
                impls)
          ;; 4. Final fallback: Object impl, if any.
          (when-let [m (get impls obj-cls)] [obj-cls m]))))))

(defn find-protocol-impl
  "Returns the implementation map for protocol implemented by x's type
  (or by x via :extend-via-metadata if enabled), or nil."
  {:added "1.2"}
  [protocol x]
  (or (when-let [hit (find-impl-for-class protocol (class x))]
        (second hit))
      (when (:extend-via-metadata protocol)
        (when-let [m (meta x)]
          (let [proto-ns (str (:ns protocol))]
            (loop [acc {}, ks (seq (keys (:method-map protocol)))]
              (if (nil? ks)
                acc
                ;; Meta-extension keys are namespace-qualified
                ;; `:proto-ns/method-name`, matching JVM contract.
                (let [bare-kw (first ks)
                      meta-kw (keyword proto-ns (name bare-kw))]
                  (if-let [f (get m meta-kw)]
                    (recur (assoc acc bare-kw f) (next ks))
                    nil)))))))))

(defn find-protocol-method
  "Returns the implementation function for method-key on x, or nil."
  {:added "1.2"}
  [protocol method-key x]
  (when-let [m (find-protocol-impl protocol x)]
    (get m method-key)))

(defn extends?
  "Returns true if atype extends protocol."
  {:added "1.2"}
  [protocol atype]
  (boolean
    (or (get-in protocol [:impls atype])
        (some (fn [pair]
                (let [ext-cls (first pair)]
                  (and (class? ext-cls) (class? atype) (isa? atype ext-cls))))
              (:impls protocol)))))

(defn extenders
  "Returns a collection of the types explicitly extending protocol."
  {:added "1.2"}
  [protocol]
  (keys (:impls protocol)))

(defn satisfies?
  "Returns true if x satisfies the protocol."
  {:added "1.2"}
  [protocol x]
  (boolean (find-protocol-impl protocol x)))

;;; --- per-method dispatch cache ------------------------------------------

(defn -reset-methods
  "Clear the per-method dispatch cache for every method of protocol."
  {:added "1.2"}
  [protocol]
  (doseq [pair (:method-map protocol)]
    (let [mvar (second pair)]
      (when-let [c (-> mvar meta ::method-cache)]
        (.clear c))))
  nil)

(defn -dispatch-protocol-method
  "Runtime helper for protocol method dispatch.

  Priority:
    1. If protocol is :extend-via-metadata, check x's metadata under
       meta-kw (a namespace-qualified `:proto-ns/method-name`
       keyword). Per-instance, so not cached. JVM behavior: meta
       extension overrides class-based extension.
    2. Otherwise, look up `(class x)` in the protocol's :impls map
       (with class-hierarchy walk) and cache the result in `cache`
       (a Python dict)."
  {:added "1.2"}
  [proto-var method-kw meta-kw method-name cache x rest-args]
  (let [proto @proto-var
        ;; Step 1: meta-extension wins if present.
        meta-impl (when (:extend-via-metadata proto)
                    (when-let [mm (meta x)]
                      (get mm meta-kw)))
        impl (or meta-impl
                 ;; Step 2: class-based dispatch with cache.
                 (let [cls (class x)
                       cached (.get cache cls cache)]
                   (if (identical? cached cache)
                     (let [hit (find-impl-for-class proto cls)
                           m (when hit (second hit))
                           f (when m (get m method-kw))]
                       (.__setitem__ cache cls f)
                       f)
                     cached)))]
    (if impl
      (apply impl x rest-args)
      (throw (IllegalArgumentException.
              (str "No implementation of method: " method-name
                   " of protocol: " (:name proto)
                   " found for class: "
                   (let [cls (class x)]
                     (if cls (.-__name__ cls) "nil"))))))))

;;; --- extend / extend-type / extend-protocol -----------------------------

(defn extend
  "Implements one or more protocols for the given type.

  (extend AType
    AProtocol  {:method1 (fn [this & args] ...) :method2 ...}
    BProtocol  {...})

  After registration, dispatch caches are reset on each affected
  protocol so future calls see the new impls."
  {:added "1.2"}
  [atype & proto+mmaps]
  (when (odd? (count proto+mmaps))
    (throw (IllegalArgumentException.
            "extend expects pairs of protocol and method-map after the type")))
  (doseq [pair (partition 2 proto+mmaps)]
    (let [proto (first pair)
          mmap (second pair)]
      (when-not (protocol? proto)
        (throw (IllegalArgumentException.
                (str "extend's even args must be protocols, got: " proto))))
      (alter-var-root (:var proto) assoc-in [:impls atype] mmap)
      (-reset-methods @(:var proto)))))

(defn- -extend-target?
  "True if x is a target slot in extend-type / extend-protocol — that
  is, a class/protocol-naming Symbol, or the literal nil (which means
  'extend over the nil case')."
  [x]
  (or (nil? x) (symbol? x)))

(defn- parse-extend-type-specs
  "Walk a flat list like (Proto1 (m1 [...] body) (m2 [...] body) Proto2 ...)
  and return ([Proto1 ((m1 ...) (m2 ...))] [Proto2 (...)]).

  Targets can be a symbol (named class/protocol) or literal nil."
  [specs]
  ;; Use a sentinel to distinguish 'no current target yet' from 'current
  ;; target is nil' — both are otherwise None.
  (let [no-target ::no-target]
    (loop [out [], current-proto no-target, current-impls [], specs (seq specs)]
      (cond
        (nil? specs)
        (if (identical? current-proto no-target)
          out
          (conj out [current-proto current-impls]))

        (-extend-target? (first specs))
        (recur (if (identical? current-proto no-target)
                 out
                 (conj out [current-proto current-impls]))
               (first specs)
               []
               (next specs))

        :else
        (recur out current-proto (conj current-impls (first specs)) (next specs))))))

(defmacro extend-type
  "Implements protocol(s) for the given type via extend.

  (extend-type SomeClass
    IFoo
    (foo [this x] (* x 2))
    (bar [this] :bar)

    IBaz
    (qux [this] :qux))"
  {:added "1.2"}
  [atype & specs]
  (let [parsed (parse-extend-type-specs specs)]
    `(extend ~atype
       ~@(mapcat
           (fn [pair]
             (let [proto-sym (first pair)
                   method-defs (second pair)]
               [proto-sym
                (into1 {}
                  (map (fn [m]
                         [(keyword (str (first m)))
                          `(fn ~@(next m))])
                       method-defs))]))
           parsed))))

(defmacro extend-protocol
  "Like extend-type but takes one protocol and multiple types.

  (extend-protocol IFoo
    SomeClass
    (foo [this] :a)

    OtherClass
    (foo [this] :b))"
  {:added "1.2"}
  [proto & specs]
  (let [no-target ::no-target
        type-impls
        (loop [out [], current-type no-target, current-impls [], specs (seq specs)]
          (cond
            (nil? specs)
            (if (identical? current-type no-target)
              out
              (conj out [current-type current-impls]))

            (-extend-target? (first specs))
            (recur (if (identical? current-type no-target)
                     out
                     (conj out [current-type current-impls]))
                   (first specs)
                   []
                   (next specs))

            :else
            (recur out current-type (conj current-impls (first specs)) (next specs))))]
    `(do
       ~@(map (fn [pair]
                (let [t (first pair)
                      methods (second pair)]
                  `(extend-type ~t ~proto ~@methods)))
              type-impls))))

;;; --- defprotocol ---------------------------------------------------------

(defn- parse-protocol-opts+sigs
  "Returns [doc, opts-map, sigs-list] from the body of a defprotocol form."
  [opts+sigs]
  (let [[doc rest] (if (string? (first opts+sigs))
                     [(first opts+sigs) (next opts+sigs)]
                     [nil opts+sigs])
        [opts sigs] (loop [opts {}, args rest]
                      (if (and (seq args) (keyword? (first args)))
                        (recur (assoc opts (first args) (second args))
                               (nnext args))
                        [opts args]))]
    [doc opts sigs]))

(defn- parse-protocol-sig
  "Each sig is (mname [args*] [args*] ... \"docstring\"?). Returns
  {:name 'mname :arglists '([args*] ...) :doc d?}."
  [sig]
  (let [mname (first sig)
        rest-of (next sig)
        [arglists doc] (if (string? (last rest-of))
                         [(butlast rest-of) (last rest-of)]
                         [rest-of nil])]
    {:name (list 'quote mname)
     :arglists (list 'quote (vec arglists))
     :doc doc}))

(defmacro defprotocol
  "Defines a Clojure protocol with the given name and method signatures.

  (defprotocol IFoo
    \"docstring\"
    (foo [this] [this x] \"method docstring\")
    (bar [this y]))

  Generates one fn per method that dispatches on the class of the
  first argument. Use extend / extend-type / extend-protocol to add
  implementations.

  Supported opt: :extend-via-metadata — when true, dispatch falls back to
  metadata on the value if the class isn't extended."
  {:added "1.2"}
  [proto-name & opts+sigs]
  (let [parsed (parse-protocol-opts+sigs opts+sigs)
        doc (first parsed)
        opts (second parsed)
        sigs (last parsed)
        method-syms (map first sigs)
        sig-map (into1 {}
                  (map (fn [sig]
                         [(keyword (str (first sig)))
                          (parse-protocol-sig sig)])
                       sigs))]
    `(do
       (defonce ~proto-name nil)
       (alter-var-root (var ~proto-name)
         (constantly
           {:name '~proto-name
            :var (var ~proto-name)
            :ns '~(ns-name *ns*)
            :doc ~doc
            :sigs ~sig-map
            :method-map {}
            :impls {}
            :extend-via-metadata ~(:extend-via-metadata opts false)}))
       ~@(map (fn [mname]
                (let [mkw (keyword (str mname))
                      ;; Qualified keyword for :extend-via-metadata lookups.
                      ;; Matches JVM's `:protocol-ns/method-name` convention.
                      meta-kw (keyword (str (ns-name *ns*)) (str mname))
                      mstr (str mname)]
                  `(let [cache# (py.__builtins__/dict)]
                     (defn ~mname
                       ([x#] (-dispatch-protocol-method (var ~proto-name)
                                                       ~mkw ~meta-kw ~mstr cache# x# nil))
                       ([x# & rest#] (-dispatch-protocol-method (var ~proto-name)
                                                                 ~mkw ~meta-kw ~mstr cache# x# rest#)))
                     (alter-meta! (var ~mname) assoc ::method-cache cache#))))
              method-syms)
       (alter-var-root (var ~proto-name)
         assoc :method-map
         ~(into1 {}
            (map (fn [mname]
                   [(keyword (str mname)) `(var ~mname)])
                 method-syms)))
       (var ~proto-name))))


;;;;;;;;;;;;;;;;;;;;;;;;;;;; reify / deftype ;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;;
;; JVM core_deftype.clj 38-507. reify generates an anonymous one-shot
;; class; deftype generates a named class with explicit fields. Both
;; can implement protocols.
;;
;; In Python we lean on the `type(name, bases, attrs)` builtin to build
;; classes at runtime. Each method body is a Clojure fn that takes
;; `this` as its first argument. Methods are attached as class
;; attributes (so `(.method inst args)` and Python's bound-method
;; mechanics work) AND registered as protocol impls via extend (so
;; `(proto-method inst args)` works through the dispatch cache).
;;
;; Adaptations from JVM:
;;   - No Java interface generation. Method dispatch for protocols
;;     goes through the :impls map; `.method` is a plain Python attr
;;     lookup.
;;   - definterface skipped (JVM-only — Python uses ABCs).
;;   - defrecord skipped for now — needs IPersistentMap implementation
;;     plus value equality / hash; coming in a follow-up slice.
;;   - Type hints / primitive args ignored (Python is fully dynamic).
;;
;; Field-name binding: deftype auto-wraps each method body in a
;; (let [field1 (.-field1 this) ...] body) so users can refer to
;; fields directly by name. JVM does this via compiler magic; we do
;; it via macroexpansion.

(defn -build-type
  "Runtime helper: build a Python class with the given name, base
  classes, and attribute map (a Clojure map of attr-name strings →
  values). Returns the new class."
  {:added "1.2"}
  [tname bases attrs-map]
  (let [pydict (py.__builtins__/dict)]
    (doseq [pair attrs-map]
      (.__setitem__ pydict (key pair) (val pair)))
    (py.__builtins__/type tname (py.__builtins__/tuple bases) pydict)))

(defn- -wrap-arity-with-field-let
  "Given an arity form ([args*] body*) and a field-symbol vector,
  rewrite to ([args*] (let [field1 (.-field1 this) ...] body*)).
  `this` is the first param of the arglist."
  [field-syms arity-form]
  (let [args (first arity-form)
        body (next arity-form)
        this-sym (first args)
        bindings (mapcat (fn [f]
                           [f (list '. this-sym (symbol (str "-" f)))])
                         field-syms)]
    (if (seq field-syms)
      `(~args (let [~@bindings] ~@body))
      `(~args ~@body))))

(defn- -rewrite-method-for-deftype
  "Rewrite (mname [args*] body*) or (mname ([args] body)+) so each
  arity binds the deftype's field names to instance attribute lookups."
  [field-syms method-form]
  (let [mname (first method-form)
        rest-of (next method-form)]
    (cond
      ;; Single arity: (mname [args] body...)
      (vector? (first rest-of))
      (cons mname (-wrap-arity-with-field-let field-syms rest-of))
      ;; Multi-arity: (mname ([args] body) ([args] body) ...)
      :else
      (cons mname
            (map (fn [arity] (-wrap-arity-with-field-let field-syms arity))
                 rest-of)))))

(defn- -emit-method-attrs
  "From parsed protocol-method specs, build a flat seq of [attr-name
  fn-form] pairs that go into the class's attribute dict. `field-syms`
  optionally bind field names inside each method body."
  [parsed field-syms]
  (mapcat
    (fn [pair]
      (mapcat
        (fn [m]
          (let [rewritten (if (seq field-syms)
                            (-rewrite-method-for-deftype field-syms m)
                            m)]
            [(str (first m)) `(fn ~@(next rewritten))]))
        (second pair)))
    parsed))

(defn- -emit-extend-form
  "Build an (extend cls Proto1 {:m1 (fn ...)} Proto2 {:m2 ...}) form
  from parsed specs. Reuses field-syms-rewriting for deftype."
  [cls-form parsed field-syms]
  `(extend ~cls-form
     ~@(mapcat
         (fn [pair]
           (let [proto-sym (first pair)
                 method-defs (second pair)]
             [proto-sym
              (apply hash-map
                (mapcat (fn [m]
                          (let [rewritten (if (seq field-syms)
                                            (-rewrite-method-for-deftype field-syms m)
                                            m)]
                            [(keyword (str (first m)))
                             `(fn ~@(next rewritten))]))
                        method-defs))]))
         parsed)))

(defmacro reify
  "Creates an anonymous instance of a new type that satisfies the given
  protocols.

  (reify
    IFoo
    (foo [this] :foo)
    IBar
    (bar [this x] (* x 2)))

  Method bodies close over the surrounding lexical environment. Each
  method takes `this` as its first parameter explicitly."
  {:added "1.2"}
  [& specs]
  (let [parsed (parse-extend-type-specs specs)
        gname (str (gensym "reify_"))
        cls-sym (gensym "cls_")
        method-attr-pairs (-emit-method-attrs parsed nil)
        extend-form (when (seq parsed)
                      (-emit-extend-form cls-sym parsed nil))]
    `(let [~cls-sym (-build-type ~gname
                                  [py.__builtins__/object]
                                  ~(if (seq method-attr-pairs)
                                     `(hash-map ~@method-attr-pairs)
                                     '{}))]
       ~@(when extend-form [extend-form])
       (~cls-sym))))

(defmacro deftype
  "Creates a new type with the given name and fields, optionally
  satisfying the given protocols.

  (deftype Point [x y]
    IPoint
    (mag [this] (Math/sqrt (+ (* x x) (* y y)))))

  Field names are auto-bound inside method bodies, so `x` and `y`
  refer to the instance's attributes. To create instances use the
  constructor (Point. 3 4) or the factory (->Point 3 4).

  Adaptations: JVM type hints on fields are accepted but ignored;
  Python's class system is fully dynamic. The :volatile-mutable /
  :unsynchronized-mutable field flags are also accepted but no-ops
  (Python attrs are always mutable)."
  {:added "1.2"}
  [Name fields & specs]
  (let [parsed (parse-extend-type-specs specs)
        ;; Strip metadata from field symbols (JVM uses ^type hints we ignore).
        field-syms (vec (map #(with-meta % nil) fields))
        method-attr-pairs (-emit-method-attrs parsed field-syms)
        ;; __init__ assigns each constructor arg to the matching attr.
        init-fn `(fn [~'this ~@field-syms]
                   ~@(map (fn [f]
                            `(py.__builtins__/setattr ~'this ~(str f) ~f))
                          field-syms)
                   nil)
        attrs `(hash-map "__init__" ~init-fn ~@method-attr-pairs)
        factory-name (symbol (str "->" Name))]
    `(do
       (def ~Name (-build-type ~(str Name)
                                [py.__builtins__/object]
                                ~attrs))
       ~(when (seq parsed)
          (-emit-extend-form Name parsed field-syms))
       (defn ~factory-name
         ~(str "Positional factory for class " Name ".")
         ~field-syms
         (~Name ~@field-syms))
       (var ~Name))))
