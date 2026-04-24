;; clojure.core — ported from ~/oss/clojure/src/clj/clojure/core.clj.
;; Embedded via include_str! and loaded at module init.

;; --- Forward declarations (core.clj lines 13-14) ---
(def unquote)
(def unquote-splicing)

;; --- list (line 20) ---
(def
  list
  (fn* list [& items] (clojure.lang.RT/list-from-seq items)))

;; --- cons (line 29) ---
(def
  cons
  (fn* cons [x seq] (clojure.lang.RT/cons x seq)))

;; --- Bootstrap let, loop, fn (lines 31-47) ---
(def
  ^{:macro true}
  let (fn* let [&form &env & decl] (cons 'let* decl)))

(def
  ^{:macro true}
  loop (fn* loop [&form &env & decl] (cons 'loop* decl)))

(def
  ^{:macro true}
  fn (fn* fn [&form &env & decl] (cons 'fn* decl)))

;; --- first, next, rest (lines 49-73) ---
(def
  first (fn* first [coll] (clojure.lang.RT/first coll)))

(def
  next (fn* next [x] (clojure.lang.RT/next x)))

(def
  rest (fn* rest [x] (clojure.lang.RT/more x)))

;; --- conj (lines 75-91) ---
(def
  conj
  (fn* conj
    ([] [])
    ([coll] coll)
    ([coll x] (clojure.lang.RT/conj coll x))
    ([coll x & xs]
     (if xs
       (recur (clojure.lang.RT/conj coll x) (first xs) (next xs))
       (clojure.lang.RT/conj coll x)))))

;; --- second, ffirst, nfirst, fnext, nnext (lines 93-126) ---
(def second (fn* second [x] (first (next x))))
(def ffirst (fn* ffirst [x] (first (first x))))
(def nfirst (fn* nfirst [x] (next (first x))))
(def fnext  (fn* fnext  [x] (first (next x))))
(def nnext  (fn* nnext  [x] (next (next x))))

;; --- seq (lines 128-139) ---
(def seq (fn* seq [coll] (clojure.lang.RT/seq coll)))

;; --- instance? and concrete type predicates (lines 141-181) ---
;; Vanilla's `(instance? cls x)` uses JVM reflection against a Class. In our
;; port the specialized predicates are implemented directly via RT, since we
;; don't have a generic "does type X implement protocol Y" symbol-driven
;; predicate yet.
(def seq?    (fn* seq?    [x] (clojure.lang.RT/instance-seq? x)))
(def char?   (fn* char?   [x] (clojure.lang.RT/instance-char? x)))
(def string? (fn* string? [x] (clojure.lang.RT/instance-string? x)))
(def map?    (fn* map?    [x] (clojure.lang.RT/instance-map? x)))
(def vector? (fn* vector? [x] (clojure.lang.RT/instance-vector? x)))

;; --- apply (placed early; syntax-quote expansions call it) ---
(def
  apply
  (fn* apply
    ([f args] (clojure.lang.RT/apply f args))
    ([f x args] (clojure.lang.RT/apply f x args))
    ([f x y args] (clojure.lang.RT/apply f x y args))
    ([f x y z args] (clojure.lang.RT/apply f x y z args))
    ([f a b c d & args]
     ;; Flatten the final variadic into one tail seq.
     (clojure.lang.RT/apply f a b c d (clojure.lang.RT/apply clojure.lang.RT/concat args)))))

;; --- concat (eager; placed early for syntax-quote) ---
(def
  concat
  (fn* concat [& seqs] (apply clojure.lang.RT/concat seqs)))

;; --- assoc (lines 183-201) ---
(def
  assoc
  (fn* assoc
    ([map key val] (clojure.lang.RT/assoc map key val))
    ([map key val & kvs]
     (let [ret (clojure.lang.RT/assoc map key val)]
       (if kvs
         (if (next kvs)
           (recur ret (first kvs) (second kvs) (nnext kvs))
           (clojure.lang.RT/throw-iae
             "assoc expects even number of arguments after map/vector, found odd number"))
         ret)))))

;; --- meta, with-meta (lines 203-220) ---
;; rt/meta is nil-safe and handles ClojureNamespace directly, so we don't
;; gate on `instance-imeta?` — namespaces don't implement IMeta but still
;; carry metadata via their `__clj_ns_meta__` dunder.
(def meta
  (fn* meta [x] (clojure.lang.RT/meta x)))

(def with-meta
  (fn* with-meta [x m]
    (clojure.lang.RT/with-meta x m)))

;; --- assert-valid-fdecl (lines 222-223) ---
(def
  ^{:private true :dynamic true}
  assert-valid-fdecl (fn* assert-valid-fdecl [fdecl]))

;; --- sigs (lines 225-261) ---
;; JVM-specific :tag resolution (clojure.lang.Compiler$HostExpr) stripped —
;; the Python port ignores :tag metadata entirely in Phase A.
(def
  ^{:private true}
  sigs
  (fn* sigs [fdecl]
    (assert-valid-fdecl fdecl)
    (let [asig
          (fn* asig [fdecl]
            (let [arglist (first fdecl)
                  body (next fdecl)]
              (if (map? (first body))
                (if (next body)
                  (with-meta arglist (conj (if (meta arglist) (meta arglist) {}) (first body)))
                  arglist)
                arglist)))]
      (if (seq? (first fdecl))
        (loop [ret [] fdecls fdecl]
          (if fdecls
            (recur (conj ret (asig (first fdecls))) (next fdecls))
            (seq ret)))
        (list (asig fdecl))))))

;; --- last, butlast (lines 263-283) ---
(def
  last
  (fn* last [s]
    (if (next s)
      (recur (next s))
      (first s))))

(def
  butlast
  (fn* butlast [s]
    (loop [ret [] s s]
      (if (next s)
        (recur (conj ret (first s)) (next s))
        (seq ret)))))

;; --- defn (lines 285-338) ---
;; Stripped: :inline inliner rewrite (JVM clojure.lang.Symbol/intern +
;; name-concat) removed — we don't honor :inline in Phase A.
(def
  ^{:doc "Same as (def name (fn [params*] exprs*)) or (def name (fn ([params*] exprs*)+)) with any doc-string or attrs added to the var metadata."
    :arglists '([name doc-string? attr-map? [params*] prepost-map? body]
                [name doc-string? attr-map? ([params*] prepost-map? body)+ attr-map?])}
  defn (fn* defn [&form &env name & fdecl]
         (if (clojure.lang.RT/instance-symbol? name)
           nil
           (clojure.lang.RT/throw-iae "First argument to defn must be a symbol"))
         (let [m (if (string? (first fdecl))
                   {:doc (first fdecl)}
                   {})
               fdecl (if (string? (first fdecl))
                       (next fdecl)
                       fdecl)
               m (if (map? (first fdecl))
                   (conj m (first fdecl))
                   m)
               fdecl (if (map? (first fdecl))
                       (next fdecl)
                       fdecl)
               fdecl (if (vector? (first fdecl))
                       (list fdecl)
                       fdecl)
               m (if (map? (last fdecl))
                   (conj m (last fdecl))
                   m)
               fdecl (if (map? (last fdecl))
                       (butlast fdecl)
                       fdecl)
               m (conj {:arglists (list 'quote (sigs fdecl))} m)
               m (conj (if (meta name) (meta name) {}) m)]
           (list 'def (with-meta name m)
                 (cons 'fn fdecl)))))

(clojure.lang.RT/set-macro (var defn))

;; --- Collection constructors block (lines 340-437).
;; `to-array`, `cast`, `sorted-map`/`-by`, `sorted-set`/`-by` are deferred
;; (no red-black tree yet, and Python has no direct Java-array analogue). ---

(defn vector
  "Creates a new vector containing the args."
  [& items]
  (clojure.lang.RT/apply clojure.lang.RT/vector items))

(defn vec
  "Creates a new vector containing the contents of coll. Java arrays
  will be aliased and should not be modified."
  [coll]
  (if (vector? coll)
    coll
    (loop [v [] s (seq coll)]
      (if s
        (recur (conj v (first s)) (next s))
        v))))

(defn hash-map
  "keyval => key val
  Returns a new hash map with supplied mappings.  If any keys are
  equal, they are handled as if by repeated uses of assoc."
  [& keyvals]
  (clojure.lang.RT/apply clojure.lang.RT/hash-map keyvals))

(defn hash-set
  "Returns a new hash set with supplied keys.  Any equal keys are
  handled as if by repeated uses of conj."
  [& keys]
  (clojure.lang.RT/apply clojure.lang.RT/hash-set keys))

(defn nil?
  "Returns true if x is nil, false otherwise."
  [x] (clojure.lang.RT/identical? x nil))

;; --- defmacro (lines 446-493) ---
(def
  ^{:doc "Like defn, but the resulting function name is declared as a macro."
    :arglists '([name doc-string? attr-map? [params*] body]
                [name doc-string? attr-map? ([params*] body)+ attr-map?])}
  defmacro (fn* defmacro [&form &env name & args]
             (let [prefix (loop [p (list name) args args]
                            (let [f (first args)]
                              (if (string? f)
                                (recur (cons f p) (next args))
                                (if (map? f)
                                  (recur (cons f p) (next args))
                                  p))))
                   fdecl (loop [fd args]
                           (if (string? (first fd))
                             (recur (next fd))
                             (if (map? (first fd))
                               (recur (next fd))
                               fd)))
                   fdecl (if (vector? (first fdecl))
                           (list fdecl)
                           fdecl)
                   add-implicit-args (fn* add-implicit-args [fd]
                                       (let [args (first fd)]
                                         (cons (vec (cons '&form (cons '&env args))) (next fd))))
                   add-args (fn* add-args [acc ds]
                              (if (nil? ds)
                                acc
                                (let [d (first ds)]
                                  (if (map? d)
                                    (conj acc d)
                                    (recur (conj acc (add-implicit-args d)) (next ds))))))
                   fdecl (seq (add-args [] fdecl))
                   decl (loop [p prefix d fdecl]
                          (if p
                            (recur (next p) (cons (first p) d))
                            d))]
               (cons 'clojure.core/defn
                     (cons (with-meta name
                             (conj (if (meta name) (meta name) {}) {:macro true}))
                           (next decl))))))

(clojure.lang.RT/set-macro (var defmacro))

;; --- when, when-not (lines 495-505) ---
(defmacro when
  "Evaluates test. If logical true, evaluates body in an implicit do."
  [test & body]
  (list 'if test (cons 'do body)))

(defmacro when-not
  "Evaluates test. If logical false, evaluates body in an implicit do."
  [test & body]
  (list 'if test nil (cons 'do body)))

;; --- false?, true?, boolean?, not, some?, any? (lines 507-544) ---
(defn false?
  "Returns true if x is the value false, false otherwise."
  [x] (clojure.lang.RT/identical? x false))

(defn true?
  "Returns true if x is the value true, false otherwise."
  [x] (clojure.lang.RT/identical? x true))

(defn boolean?
  "Return true if x is a Boolean"
  [x] (clojure.lang.RT/instance-bool? x))

(defn not
  "Returns true if x is logical false, false otherwise."
  [x] (if x false true))

(defn some?
  "Returns true if x is not nil, false otherwise."
  [x] (not (nil? x)))

(defn any?
  "Returns true given any argument."
  [x] true)

;; --- str (lines 546-561) — rewrites StringBuilder to RT/str-concat ---
(defn str
  "With no args, returns the empty string. With one arg x, returns
  x.toString(). (str nil) returns the empty string. With more than
  one arg, returns the concatenation of the str values of the args."
  ([] "")
  ([x]
   (if (nil? x) "" (clojure.lang.RT/to-string x)))
  ([x & ys]
   (loop [acc (str x) s (seq ys)]
     (if (nil? s)
       acc
       (recur (clojure.lang.RT/str-concat acc (str (first s))) (next s))))))

;; --- symbol?, keyword? (lines 564-574) ---
(defn symbol?
  "Return true if x is a Symbol"
  [x] (clojure.lang.RT/instance-symbol? x))

(defn keyword?
  "Return true if x is a Keyword"
  [x] (clojure.lang.RT/instance-keyword? x))

;; --- cond (lines 576-589) ---
(defmacro cond
  "Takes a set of test/expr pairs. It evaluates each test one at a
  time. If a test returns logical true, cond evaluates and returns
  the value of the corresponding expr and doesn't evaluate any of the
  other tests or exprs. (cond) returns nil."
  [& clauses]
  (when clauses
    (list 'if (first clauses)
          (if (next clauses)
            (second clauses)
            (clojure.lang.RT/throw-iae
              "cond requires an even number of forms"))
          (cons 'clojure.core/cond (next (next clauses))))))

;; --- symbol (lines ~591-604) ---
(defn symbol
  "Returns a Symbol with the given namespace and name."
  ([name]
   (cond
     (symbol? name) name
     (string? name) (clojure.lang.RT/symbol name)
     :else (clojure.lang.RT/throw-iae "no conversion to symbol")))
  ([ns name] (clojure.lang.RT/symbol ns name)))

;; --- gensym (lines 606-613) ---
(defn gensym
  "Returns a new symbol with a unique name. If a prefix string is
  supplied, the name is prefix# where # is some unique number. If
  prefix is not supplied, the prefix is 'G__'."
  ([] (gensym "G__"))
  ([prefix-string]
   (clojure.lang.RT/symbol
     (clojure.lang.RT/str-concat prefix-string (clojure.lang.RT/next-id)))))

;; --- keyword (lines 616-625) ---
(defn keyword
  "Returns a Keyword with the given namespace and name."
  ([name]
   (cond
     (keyword? name) name
     (symbol? name) (clojure.lang.RT/keyword (.-ns name) (.-name name))
     (string? name) (clojure.lang.RT/keyword name)
     :else (clojure.lang.RT/throw-iae "no conversion to keyword")))
  ([ns name] (clojure.lang.RT/keyword ns name)))

;; --- find-keyword (lines 627-638) ---
(defn find-keyword
  "Returns a Keyword with the given namespace and name if one already
  exists. This function will not intern a new keyword. If the keyword
  has not already been interned, it will return nil."
  ([name]
   (cond
     (keyword? name) name
     (symbol? name) (clojure.lang.RT/find-keyword (.-ns name) (.-name name))
     (string? name) (clojure.lang.RT/find-keyword name)))
  ([ns name] (clojure.lang.RT/find-keyword ns name)))

;; --- spread (private; line 641) ---
(defn ^:private spread
  [arglist]
  (cond
    (nil? arglist) nil
    (nil? (next arglist)) (seq (first arglist))
    :else (cons (first arglist) (spread (next arglist)))))

;; --- list* (lines 650-660) ---
(defn list*
  "Creates a new seq containing the items prepended to the rest, the
  last of which will be treated as a sequence."
  ([args] (seq args))
  ([a args] (cons a args))
  ([a b args] (cons a (cons b args)))
  ([a b c args] (cons a (cons b (cons c args))))
  ([a b c d & more]
   (cons a (cons b (cons c (cons d (spread more)))))))

;; `apply` is defined earlier (line 75) for syntax-quote bootstrap; skip its
;; original slot at vanilla line 662.

;; --- vary-meta (lines 677-683) ---
(defn vary-meta
  "Returns an object of the same type and value as obj, with
  (apply f (meta obj) args) as its metadata."
  [obj f & args]
  (with-meta obj (apply f (meta obj) args)))

;; --- lazy-seq (line 685) ---
;; Vanilla: `(new clojure.lang.LazySeq (^:once fn* [] body))`. We avoid the
;; `new` special form by constructing via RT, which also ensures the LazySeq
;; pyclass is the canonical one installed at module init.
(defmacro lazy-seq
  "Takes a body of expressions that returns an ISeq or nil, and yields
  a Seqable object that will invoke the body only the first time seq
  is called, and will cache the result and return it on all subsequent
  seq calls."
  [& body]
  (list 'clojure.lang.RT/lazy-seq (list 'fn* [] (cons 'do body))))

;; --- chunk-buffer / chunk-append / chunk / chunk-first / chunk-rest /
;;     chunk-next / chunk-cons / chunked-seq? (lines 694-718) ---
(defn ^:private chunk-buffer [capacity]
  (clojure.lang.RT/chunk-buffer capacity))

(defn ^:private chunk-append [b x]
  (clojure.lang.RT/chunk-append b x))

(defn ^:private chunk [b]
  (clojure.lang.RT/chunk b))

(defn ^:private chunk-first [s]
  (clojure.lang.RT/chunk-first s))

(defn ^:private chunk-rest [s]
  (clojure.lang.RT/chunk-rest s))

(defn ^:private chunk-next [s]
  (clojure.lang.RT/chunk-next s))

(defn ^:private chunk-cons [chunk rest]
  (clojure.lang.RT/chunk-cons chunk rest))

(defn chunked-seq? [s]
  (clojure.lang.RT/instance-chunked-seq? s))

;; --- concat (lines 720-745) — lazy version, shadowing the eager bootstrap
;; def above. The Rust `clojure.lang.RT/concat` remains for syntax-quote
;; expansions that need an eager concat (reader-time).
(defn concat
  "Returns a lazy seq representing the concatenation of the elements in the supplied colls."
  ([] (lazy-seq nil))
  ([x] (lazy-seq x))
  ([x y]
   (lazy-seq
     (let [s (seq x)]
       (if s
         (if (chunked-seq? s)
           (chunk-cons (chunk-first s) (concat (chunk-rest s) y))
           (cons (first s) (concat (rest s) y)))
         y))))
  ([x y & zs]
   (let [cat (fn cat [xys zs]
               (lazy-seq
                 (let [xys (seq xys)]
                   (if xys
                     (if (chunked-seq? xys)
                       (chunk-cons (chunk-first xys)
                                   (cat (chunk-rest xys) zs))
                       (cons (first xys) (cat (rest xys) zs)))
                     (when zs
                       (cat (first zs) (next zs)))))))]
     (cat (concat x y) zs))))

;; --- delay, delay?, force (lines 748-767) ---
(defmacro delay
  "Takes a body of expressions and yields a Delay object that will
  invoke the body only the first time it is forced (with force or
  deref), and will cache the result and return it on all subsequent
  force calls."
  [& body]
  (list 'clojure.lang.RT/delay (list 'fn* [] (cons 'do body))))

(defn delay?
  "Returns true if x is a Delay created with delay."
  [x] (clojure.lang.RT/instance-delay? x))

(defn force
  "If x is a Delay, returns the (possibly cached) value of its expression, else returns x."
  [x] (clojure.lang.RT/force x))

;; --- if-not (lines 769-775) ---
(defmacro if-not
  "Evaluates test. If logical false, evaluates and returns then expr,
  otherwise else expr, if supplied, else nil."
  ([test then] `(if-not ~test ~then nil))
  ([test then else]
   `(if (not ~test) ~then ~else)))

;; --- identical?, =, not=, compare (lines 777-842) ---
(defn identical?
  "Tests if 2 arguments are the same object."
  [x y] (clojure.lang.RT/identical? x y))

(defn =
  "Equality. Returns true if x equals y, false if not."
  ([x] true)
  ([x y] (clojure.lang.RT/equiv x y))
  ([x y & more]
   (if (clojure.lang.RT/equiv x y)
     (if (next more)
       (recur y (first more) (next more))
       (clojure.lang.RT/equiv y (first more)))
     false)))

(defn not=
  "Same as (not (= obj1 obj2))."
  ([x] false)
  ([x y] (not (= x y)))
  ([x y & more] (not (apply = x y more))))

;; --- compare (lines 833-842) ---
(defn compare
  "Comparator. Returns a negative number, zero, or a positive number
  when x is logically 'less than', 'equal to', or 'greater than'
  y. Same as Java x.compareTo(y) except it also works for nil, and
  compares numbers and collections in a type-independent manner. x
  must implement Comparable."
  [x y] (clojure.lang.RT/compare x y))

;; --- and, or (lines 844-866) — use quasiquote + auto-gensym ---
(defmacro and
  "Evaluates exprs one at a time, from left to right."
  ([] true)
  ([x] x)
  ([x & next]
   `(let [and# ~x]
      (if and# (and ~@next) and#))))

(defmacro or
  "Evaluates exprs one at a time, from left to right."
  ([] nil)
  ([x] x)
  ([x & next]
   `(let [or# ~x]
      (if or# or# (or ~@next)))))

;;;;;;;;;;;;;;;;;;; sequence fns  ;;;;;;;;;;;;;;;;;;;;;;;
;; Vanilla lines 868-1100. Ports focus on variadic Clojure shapes backed by
;; 2-arg RT primitives. `cast` is deferred, so the 1-arity cases of + and *
;; return their arg unchanged. `rationalize` is deferred (no Ratio type).

;; --- zero? (869), count (876), int (884), nth (891) ---
(defn zero?
  "Returns true if num is zero, else false."
  [num] (clojure.lang.RT/equiv num 0))

(defn count
  "Returns the number of items in the collection. (count nil) returns 0.
  Also works on strings, arrays, and Python sequences."
  [coll] (clojure.lang.RT/count coll))

(defn int
  "Coerce to int."
  [x] (clojure.lang.RT/coerce-int x))

(defn nth
  "Returns the value at the index. get returns nil if index out of bounds,
  nth throws an exception unless not-found is supplied."
  ([coll index] (clojure.lang.RT/nth coll index))
  ([coll index not-found] (clojure.lang.RT/nth coll index not-found)))

;; --- < (905), inc (946), > (957) ---
(defn <
  "Returns non-nil if nums are in monotonically increasing order, otherwise false."
  ([x] true)
  ([x y] (clojure.lang.RT/lt x y))
  ([x y & more]
   (if (< x y)
     (if (next more)
       (recur y (first more) (next more))
       (< y (first more)))
     false)))

(defn inc
  "Returns a number one greater than num."
  [x] (clojure.lang.RT/inc x))

(defn >
  "Returns non-nil if nums are in monotonically decreasing order, otherwise false."
  ([x] true)
  ([x y] (clojure.lang.RT/gt x y))
  ([x y & more]
   (if (> x y)
     (if (next more)
       (recur y (first more) (next more))
       (> y (first more)))
     false)))

;; --- reduce1 (977, private) ---
(defn ^:private reduce1
  "Internal reduce used by variadic arithmetic. Uses the chunked-seq fast
  path for chunked sources; short-circuit via Reduced is not honored here
  (variadic arithmetic never short-circuits)."
  ([f coll]
   (let [s (seq coll)]
     (if s
       (reduce1 f (first s) (next s))
       (f))))
  ([f val coll]
   (let [s (seq coll)]
     (if s
       (if (chunked-seq? s)
         (recur f
                (clojure.lang.RT/chunk-reduce (chunk-first s) f val)
                (chunk-next s))
         (recur f (f val (first s)) (next s)))
       val))))

;; --- reverse (995) ---
(defn reverse
  "Returns a seq of the items in coll in reverse order. Not lazy."
  [coll] (reduce1 conj () coll))

;; --- >1?, >0? (1010-1012, private) ---
(defn ^:private >1? [n] (clojure.lang.RT/gt n 1))
(defn ^:private >0? [n] (clojure.lang.RT/gt n 0))

;; --- + (1014) ---
(defn +
  "Returns the sum of nums. (+) returns 0. Does not auto-promote (Python
  ints are arbitrary-precision already)."
  ([] 0)
  ([x] x)
  ([x y] (clojure.lang.RT/add x y))
  ([x y & more] (reduce1 + (+ x y) more)))

;; --- * (1034) ---
(defn *
  "Returns the product of nums. (*) returns 1."
  ([] 1)
  ([x] x)
  ([x y] (clojure.lang.RT/multiply x y))
  ([x y & more] (reduce1 * (* x y) more)))

;; --- / (1050) ---
(defn /
  "If no denominators are supplied, returns 1/x, else returns numerator
  divided by all of the denominators."
  ([x] (/ 1 x))
  ([x y] (clojure.lang.RT/divide x y))
  ([x y & more] (reduce1 / (/ x y) more)))

;; --- - (1068) ---
(defn -
  "If no ys are supplied, returns the negation of x, else subtracts the ys
  from x and returns the result."
  ([x] (clojure.lang.RT/negate x))
  ([x y] (clojure.lang.RT/subtract x y))
  ([x y & more] (reduce1 - (- x y) more)))

;; --- <= (1088), >= (1099), == (1110) ---
(defn <=
  "Returns non-nil if nums are in monotonically non-decreasing order."
  ([x] true)
  ([x y] (clojure.lang.RT/lte x y))
  ([x y & more]
   (if (<= x y)
     (if (next more)
       (recur y (first more) (next more))
       (<= y (first more)))
     false)))

(defn >=
  "Returns non-nil if nums are in monotonically non-increasing order."
  ([x] true)
  ([x y] (clojure.lang.RT/gte x y))
  ([x y & more]
   (if (>= x y)
     (if (next more)
       (recur y (first more) (next more))
       (>= y (first more)))
     false)))

(defn ==
  "Returns non-nil if nums all have the equivalent value (type-independent)."
  ([x] true)
  ([x y] (clojure.lang.RT/equiv x y))
  ([x y & more]
   (if (== x y)
     (if (next more)
       (recur y (first more) (next more))
       (== y (first more)))
     false)))

;; --- pos? (1262), neg? (1270) — hoisted above `abs` since our compiler
;;     resolves symbol references at compile time and `abs` calls `neg?`.
(defn pos?
  "Returns true if num is greater than zero, else false."
  [num] (clojure.lang.RT/gt num 0))

(defn neg?
  "Returns true if num is less than zero, else false."
  [num] (clojure.lang.RT/lt num 0))

;; --- max (1121), min (1131), abs (1141), dec (1148) ---
(defn max
  "Returns the greatest of the nums."
  ([x] x)
  ([x y] (if (> x y) x y))
  ([x y & more] (reduce1 max (max x y) more)))

(defn min
  "Returns the least of the nums."
  ([x] x)
  ([x y] (if (< x y) x y))
  ([x y & more] (reduce1 min (min x y) more)))

(defn abs
  "Returns the absolute value of a."
  [a] (if (neg? a) (- a) a))

(defn dec
  "Returns a number one less than num."
  [x] (clojure.lang.RT/dec x))

;; --- unchecked-* variants (1153-1260)
;; Python ints don't overflow, so every unchecked-* is an alias for the
;; corresponding checked op. JVM Clojure defines these for perf on primitive
;; longs; we keep the names for 1:1 but bind them to the checked versions.
(def unchecked-inc inc)
(def unchecked-inc-int inc)
(def unchecked-dec dec)
(def unchecked-dec-int dec)
(def unchecked-negate -)
(def unchecked-negate-int -)
(def unchecked-add +)
(def unchecked-add-int +)
(def unchecked-subtract -)
(def unchecked-subtract-int -)
(def unchecked-multiply *)
(def unchecked-multiply-int *)
(defn unchecked-divide-int [x y] (clojure.lang.RT/quot x y))
(defn unchecked-remainder-int [x y] (clojure.lang.RT/rem x y))

;; --- quot (1278), rem (1285) ---
(defn quot
  "quot[ient] of dividing numerator by denominator."
  [num div] (clojure.lang.RT/quot num div))

(defn rem
  "remainder of dividing numerator by denominator."
  [num div] (clojure.lang.RT/rem num div))

;; `rationalize` (1292) is deferred — requires a Ratio type we don't have yet.

;; --- bit ops (1302-1394) ---
(defn bit-not
  "Bitwise complement."
  [x] (clojure.lang.RT/bit-not x))

(defn bit-and
  "Bitwise and."
  ([x y] (clojure.lang.RT/bit-and x y))
  ([x y & more] (reduce1 bit-and (bit-and x y) more)))

(defn bit-or
  "Bitwise or."
  ([x y] (clojure.lang.RT/bit-or x y))
  ([x y & more] (reduce1 bit-or (bit-or x y) more)))

(defn bit-xor
  "Bitwise exclusive or."
  ([x y] (clojure.lang.RT/bit-xor x y))
  ([x y & more] (reduce1 bit-xor (bit-xor x y) more)))

(defn bit-and-not
  "Bitwise and with complement."
  ([x y] (clojure.lang.RT/bit-and-not x y))
  ([x y & more] (reduce1 bit-and-not (bit-and-not x y) more)))

(defn bit-clear
  "Clear bit at index n."
  [x n] (clojure.lang.RT/bit-clear x n))

(defn bit-set
  "Set bit at index n."
  [x n] (clojure.lang.RT/bit-set x n))

(defn bit-flip
  "Flip bit at index n."
  [x n] (clojure.lang.RT/bit-flip x n))

(defn bit-test
  "Test bit at index n."
  [x n] (clojure.lang.RT/bit-test x n))

(defn bit-shift-left
  [x n] (clojure.lang.RT/bit-shift-left x n))

(defn bit-shift-right
  [x n] (clojure.lang.RT/bit-shift-right x n))

(defn unsigned-bit-shift-right
  "JVM-style logical right shift; the operand is treated as a 64-bit
  unsigned long before shifting."
  [x n] (clojure.lang.RT/unsigned-bit-shift-right x n))

;; --- Type predicates (vanilla 1394-1429) ---
(defn integer?
  "Returns true if n is an integer."
  [n] (clojure.lang.RT/integer? n))

(defn even?
  "Returns true if n is even, throws an exception if n is not an integer."
  [n]
  (if (integer? n)
    (zero? (bit-and n 1))
    (clojure.lang.RT/throw-iae
      (clojure.lang.RT/str-concat "Argument must be an integer: " n))))

(defn odd?
  "Returns true if n is odd, throws an exception if n is not an integer."
  [n] (not (even? n)))

(defn int?
  "Return true if x is a fixed-precision integer."
  [x] (clojure.lang.RT/integer? x))

(defn pos-int?
  "Return true if x is a positive fixed-precision integer."
  [x] (and (int? x) (pos? x)))

(defn neg-int?
  "Return true if x is a negative fixed-precision integer."
  [x] (and (int? x) (neg? x)))

(defn nat-int?
  "Return true if x is a non-negative fixed-precision integer."
  [x] (and (int? x) (not (neg? x))))

(defn double?
  "Return true if x is a Double (Python float)."
  [x] (clojure.lang.RT/double? x))

(defn number?
  "Returns true if x is a Number."
  [x] (clojure.lang.RT/number? x))

;; --- Identity utilities (vanilla 1436-1462) ---
(defn complement
  "Takes a fn f and returns a fn that takes the same arguments as f,
  has the same effects, if any, and returns the opposite truth value."
  [f]
  (fn
    ([] (not (f)))
    ([x] (not (f x)))
    ([x y] (not (f x y)))
    ([x y & zs] (not (apply f x y zs)))))

(defn constantly
  "Returns a function that takes any number of arguments and returns x."
  [x] (fn [& args] x))

(defn identity
  "Returns its argument."
  [x] x)

;; --- Collection access (vanilla 1462-1545) ---
(defn peek
  "For a list or queue, same as first, for a vector, same as last. If
  the collection is empty, returns nil."
  [coll] (clojure.lang.RT/peek coll))

(defn pop
  "For a list or queue, returns a new list/queue without the first
  item, for a vector, returns a new vector without the last item."
  [coll] (clojure.lang.RT/pop coll))

(defn map-entry?
  "Return true if x is a map entry (key/value pair)."
  [x] (clojure.lang.RT/instance-map-entry? x))

(defn contains?
  "Returns true if key is present in the given collection, otherwise
  returns false."
  [coll key] (clojure.lang.RT/contains? coll key))

(defn get
  "Returns the value mapped to key, not-found or nil if key not present."
  ([map key] (clojure.lang.RT/get map key))
  ([map key not-found] (clojure.lang.RT/get map key not-found)))

(defn dissoc
  "dissoc[iate]. Returns a new map of the same (hashed/sorted) type,
  that does not contain a mapping for key(s)."
  ([map] map)
  ([map key] (clojure.lang.RT/dissoc map key))
  ([map key & ks]
   (let [ret (dissoc map key)]
     (if ks
       (recur ret (first ks) (next ks))
       ret))))

(defn disj
  "disj[oin]. Returns a new set of the same (hashed/sorted) type, that
  does not contain key(s)."
  ([set] set)
  ([set key] (clojure.lang.RT/disj set key))
  ([set key & ks]
   (let [ret (disj set key)]
     (if ks
       (recur ret (first ks) (next ks))
       ret))))

(defn find
  "Returns the map entry for key, or nil if key not present."
  [map key] (clojure.lang.RT/find map key))

(defn select-keys
  "Returns a map containing only those entries in map whose key is in keys."
  [map keyseq]
  (loop [ret {} keys (seq keyseq)]
    (if keys
      (let [entry (find map (first keys))]
        (recur
          (if entry
            (conj ret entry)
            ret)
          (next keys)))
      (with-meta ret (meta map)))))

(defn keys
  "Returns a sequence of the map's keys, in the same order as (seq map)."
  [map] (clojure.lang.RT/keys map))

(defn vals
  "Returns a sequence of the map's values, in the same order as (seq map)."
  [map] (clojure.lang.RT/vals map))

(defn key
  "Returns the key of the map entry."
  [e] (.-key e))

(defn val
  "Returns the value in the map entry."
  [e] (.-val e))

;; `rseq` (vanilla 1545) is deferred — requires a Reversible protocol we
;; don't have yet.

;; --- Symbol/keyword utilities (vanilla 1548-1590) ---
(defn name
  "Returns the name String of a string, symbol or keyword."
  [x] (clojure.lang.RT/name x))

(defn namespace
  "Returns the namespace String of a symbol or keyword, or nil."
  [x] (clojure.lang.RT/namespace x))

(defn boolean
  "Coerce to boolean."
  [x] (if x true false))

(defn ident?
  "Return true if x is a symbol or keyword."
  [x] (or (symbol? x) (keyword? x)))

(defn simple-ident?
  "Return true if x is a symbol or keyword without a namespace."
  [x] (and (ident? x) (nil? (namespace x))))

(defn qualified-ident?
  "Return true if x is a symbol or keyword with a namespace."
  [x] (boolean (and (ident? x) (namespace x) true)))

(defn simple-symbol?
  "Return true if x is a symbol without a namespace."
  [x] (and (symbol? x) (nil? (namespace x))))

(defn qualified-symbol?
  "Return true if x is a symbol with a namespace."
  [x] (boolean (and (symbol? x) (namespace x) true)))

(defn simple-keyword?
  "Return true if x is a keyword without a namespace."
  [x] (and (keyword? x) (nil? (namespace x))))

(defn qualified-keyword?
  "Return true if x is a keyword with a namespace."
  [x] (boolean (and (keyword? x) (namespace x) true)))

;; --- Reduced + public reduce family ---
;; Vanilla scatters these: `reduced`/`reduced?`/`ensure-reduced`/`unreduced`
;; land around vanilla line 2610, `reduce`/`reduce-kv` around 7038. We group
;; them here so the user-facing reducer API is available as early as
;; possible — the machinery it needs (CollReduce, IKVReduce) already exists.

(defn reduced
  "Wraps x in a value that can be returned from a reduce function to
  stop the reduction and return x as the result."
  [x] (clojure.lang.RT/reduced x))

(defn reduced?
  "Returns true if x is the result of a call to reduced."
  [x] (clojure.lang.RT/reduced? x))

(defn ensure-reduced
  "If x is already reduced?, returns it, else returns (reduced x)."
  [x] (clojure.lang.RT/ensure-reduced x))

(defn unreduced
  "If x is reduced?, returns (deref x), else returns x."
  [x] (clojure.lang.RT/unreduced x))

(defn reduce
  "f should be a function of 2 arguments. If val is not supplied, returns
  the result of applying f to the first 2 items in coll, then applying f
  to that result and the 3rd item, etc. If coll contains no items, f must
  accept no arguments as well, and reduce returns the result of calling
  f with no arguments. If coll has only 1 item, it is returned and f is
  not called. If val is supplied, returns the result of applying f to val
  and the first item in coll, then applying f to that result and the 2nd
  item, etc. If coll contains no items, returns val and f is not called."
  ([f coll] (clojure.lang.RT/coll-reduce coll f))
  ([f val coll] (clojure.lang.RT/coll-reduce coll f val)))

(defn reduce-kv
  "Reduces an associative collection. f should be a function of 3
  arguments. Returns the result of applying f to init, the first key
  and the first value in coll, then applying f to that result and the
  2nd key and value, etc."
  [f init coll]
  (if (nil? coll) init (clojure.lang.RT/kv-reduce coll f init)))

;; --- Threading macros (vanilla 1595-1650). `locking` deferred (needs
;; threading primitives we don't yet have). ---

(defmacro ..
  "form => fieldName-symbol or (instanceMethodName-symbol args*)
  Expands into a member access chain: (.. x y z w) == (. (. (. x y) z) w)."
  ([x form] `(. ~x ~form))
  ([x form & more] `(.. (. ~x ~form) ~@more)))

(defmacro ->
  "Threads the expr through the forms. Inserts x as the second item in
  the first form, making a list of it if it is not a list already. If
  there are more forms, inserts the first form as the second item in
  second form, etc."
  [x & forms]
  (loop [x x, forms forms]
    (if forms
      (let [form (first forms)
            threaded (if (seq? form)
                       (with-meta `(~(first form) ~x ~@(next form)) (meta form))
                       (list form x))]
        (recur threaded (next forms)))
      x)))

(defmacro ->>
  "Threads the expr through the forms. Inserts x as the last item in
  the first form, making a list of it if it is not a list already. If
  there are more forms, inserts the first form as the last item in
  second form, etc."
  [x & forms]
  (loop [x x, forms forms]
    (if forms
      (let [form (first forms)
            threaded (if (seq? form)
                       (with-meta `(~(first form) ~@(next form) ~x) (meta form))
                       (list form x))]
        (recur threaded (next forms)))
      x)))

;; --- Binding / conditional macros (vanilla 1889-1950).
;; vanilla's `assert-args` threads through &form/meta for line-aware errors;
;; we inline simpler checks here since we don't yet wire the full source-map
;; into macro expansion. ---

(defmacro ^:private ^{:arglists '([& pairs])} assert-args
  [& pairs]
  `(do (when-not ~(first pairs)
         (clojure.lang.RT/throw-iae ~(second pairs)))
       ~(let [more (nnext pairs)]
          (when more (list* `assert-args more)))))

(defmacro if-let
  "bindings => binding-form test
  If test is true, evaluates then with binding-form bound to the value
  of test, if not, yields else."
  ([bindings then] `(if-let ~bindings ~then nil))
  ([bindings then else]
   (assert-args
     (vector? bindings) "if-let requires a vector for its binding"
     (= 2 (count bindings)) "if-let requires exactly 2 forms in binding vector")
   (let [form (bindings 0) tst (bindings 1)]
     `(let [temp# ~tst]
        (if temp#
          (let [~form temp#] ~then)
          ~else)))))

(defmacro when-let
  "bindings => binding-form test
  When test is true, evaluates body with binding-form bound to the value of test."
  [bindings & body]
  (assert-args
    (vector? bindings) "when-let requires a vector for its binding"
    (= 2 (count bindings)) "when-let requires exactly 2 forms in binding vector")
  (let [form (bindings 0) tst (bindings 1)]
    `(let [temp# ~tst]
       (when temp#
         (let [~form temp#] ~@body)))))

(defmacro if-some
  "bindings => binding-form test
  If test is not nil, evaluates then with binding-form bound to the value
  of test, if not, yields else."
  ([bindings then] `(if-some ~bindings ~then nil))
  ([bindings then else]
   (assert-args
     (vector? bindings) "if-some requires a vector for its binding"
     (= 2 (count bindings)) "if-some requires exactly 2 forms in binding vector")
   (let [form (bindings 0) tst (bindings 1)]
     `(let [temp# ~tst]
        (if (nil? temp#)
          ~else
          (let [~form temp#] ~then))))))

(defmacro when-some
  "bindings => binding-form test
  When test is not nil, evaluates body with binding-form bound to the value of test."
  [bindings & body]
  (assert-args
    (vector? bindings) "when-some requires a vector for its binding"
    (= 2 (count bindings)) "when-some requires exactly 2 forms in binding vector")
  (let [form (bindings 0) tst (bindings 1)]
    `(let [temp# ~tst]
       (if (nil? temp#)
         nil
         (let [~form temp#] ~@body)))))

;; --- Atom / deref family (vanilla ~line 2200-2300, hoisted) ---
;; Atom is the go-to reference type for uncoordinated, synchronous state.
;; Backed by ArcSwap on the Rust side: reads are lock-free, writes CAS.

(defn atom
  "Creates and returns an Atom with an initial value of x."
  [x] (clojure.lang.RT/atom x))

(defn deref
  "Also reader macro: @ref/@agent/@var/@atom/@delay/@future/@promise.
  Dereferences the ref; returns the current value."
  [ref] (clojure.lang.RT/deref ref))

(defn swap!
  "Atomically swaps the value of atom to be (apply f current-value-of-atom args).
  Returns the value that was swapped in."
  ([a f] (clojure.lang.RT/swap-bang a f))
  ([a f x] (clojure.lang.RT/swap-bang a f x))
  ([a f x y] (clojure.lang.RT/swap-bang a f x y))
  ([a f x y & args]
   (clojure.lang.RT/apply clojure.lang.RT/swap-bang a f x y args)))

(defn swap-vals!
  "Atomically swaps the value of atom to be (apply f current-value-of-atom args).
  Returns [old-value new-value]."
  ([a f] (clojure.lang.RT/swap-vals-bang a f))
  ([a f x] (clojure.lang.RT/swap-vals-bang a f x))
  ([a f x y] (clojure.lang.RT/swap-vals-bang a f x y))
  ([a f x y & args]
   (clojure.lang.RT/apply clojure.lang.RT/swap-vals-bang a f x y args)))

(defn reset!
  "Sets the value of atom to newval without regard for the current value.
  Returns newval."
  [a newval] (clojure.lang.RT/reset-bang a newval))

(defn reset-vals!
  "Sets the value of atom to newval. Returns [old new]."
  [a newval] (clojure.lang.RT/reset-vals-bang a newval))

(defn compare-and-set!
  "Atomically sets the value of atom to newval if and only if the
  current value of the atom is equal to oldval. Returns true if set
  happened, else false."
  [a oldval newval] (clojure.lang.RT/compare-and-set-bang a oldval newval))

;; --- Volatile (vanilla ~line 2560, hoisted) ---
;; Single-thread-owned mutable cell. Used for transducer state. NOT
;; thread-safe by design — `vswap!` is read + apply + write, no CAS.

(defn volatile!
  "Creates and returns a Volatile with an initial value of val."
  [val] (clojure.lang.RT/volatile val))

(defn volatile?
  "Returns true if x is a Volatile."
  [x] (clojure.lang.RT/volatile? x))

(defn vreset!
  "Sets the value of volatile to newval without regard for the current
  value. Returns newval."
  [vol newval] (clojure.lang.RT/vreset vol newval))

(defmacro vswap!
  "Non-atomically swaps the value of the volatile as if:
  (apply f current-value-of-vol args). Returns the value that was
  swapped in."
  [vol f & args]
  `(clojure.lang.RT/vswap ~vol ~f ~@args))

;;;;;;;;;;;;;;;;;;;;;; sequence transforms (vanilla ~2574-2850) ;;;;;;;;;;;;;;;;;;;;;;

;; --- Closure utilities (vanilla 2575-2665) ---

(defn comp
  "Takes a set of functions and returns a fn that is the composition
  of those fns. The returned fn takes a variable number of args,
  applies the rightmost fn to the args, then each fn in reverse order
  to the result in succession."
  ([] identity)
  ([f] f)
  ([f g] (fn [& args] (f (apply g args))))
  ([f g h] (fn [& args] (f (g (apply h args)))))
  ([f1 f2 f3 & fs]
   (let [fs (reverse (list* f1 f2 f3 fs))]
     (fn [& args]
       (loop [ret (apply (first fs) args) fs (next fs)]
         (if fs
           (recur ((first fs) ret) (next fs))
           ret))))))

(defn juxt
  "Takes a set of functions and returns a fn that is the juxtaposition
  of those fns. The returned fn takes a variable number of args, and
  returns a vector containing the result of applying each fn to the
  args (left-to-right)."
  ([f] (fn [& args] [(apply f args)]))
  ([f g] (fn [& args] [(apply f args) (apply g args)]))
  ([f g h] (fn [& args] [(apply f args) (apply g args) (apply h args)]))
  ([f g h & fs]
   (let [fs (list* f g h fs)]
     (fn [& args]
       (reduce (fn [acc ff] (conj acc (apply ff args))) [] fs)))))

(defn partial
  "Takes a function f and fewer than the normal arguments to f, and
  returns a fn that takes a variable number of additional args. When
  called, the returned function calls f with args + additional args."
  ([f] f)
  ([f arg1] (fn [& args] (apply f arg1 args)))
  ([f arg1 arg2] (fn [& args] (apply f arg1 arg2 args)))
  ([f arg1 arg2 arg3] (fn [& args] (apply f arg1 arg2 arg3 args)))
  ([f arg1 arg2 arg3 & more]
   (fn [& args] (apply f arg1 arg2 arg3 (concat more args)))))

;; --- Predicate combinators (vanilla 2692-2728) ---

(defn every?
  "Returns true if (pred x) is logical true for every x in coll, else false."
  [pred coll]
  (cond
    (nil? (seq coll)) true
    (pred (first coll)) (recur pred (next coll))
    :else false))

(defn not-every?
  "Returns false if (pred x) is logical true for every x in coll, else true."
  [pred coll] (not (every? pred coll)))

(defn some
  "Returns the first logical true value of (pred x) for any x in coll,
  else nil."
  [pred coll]
  (when (seq coll)
    (or (pred (first coll)) (recur pred (next coll)))))

(defn not-any?
  "Returns false if (pred x) is logical true for any x in coll, else true."
  [pred coll] (not (some pred coll)))

;; --- map / filter / remove / keep (vanilla ~2731-2820) ---

(defn map
  "Returns a lazy sequence consisting of the result of applying f to
  the set of first items of each coll, followed by applying f to the
  set of second items in each coll, until any one of the colls is
  exhausted."
  ([f coll]
   (lazy-seq
     (when-let [s (seq coll)]
       (if (chunked-seq? s)
         (let [c (chunk-first s)
               size (count c)
               b (chunk-buffer size)]
           (loop [i 0]
             (if (< i size)
               (do (chunk-append b (f (nth c i)))
                   (recur (inc i)))
               (chunk-cons (chunk b) (map f (chunk-rest s))))))
         (cons (f (first s)) (map f (rest s)))))))
  ([f c1 c2]
   (lazy-seq
     (let [s1 (seq c1) s2 (seq c2)]
       (when (and s1 s2)
         (cons (f (first s1) (first s2))
               (map f (rest s1) (rest s2)))))))
  ([f c1 c2 c3]
   (lazy-seq
     (let [s1 (seq c1) s2 (seq c2) s3 (seq c3)]
       (when (and s1 s2 s3)
         (cons (f (first s1) (first s2) (first s3))
               (map f (rest s1) (rest s2) (rest s3))))))))

(defn filter
  "Returns a lazy sequence of the items in coll for which (pred item)
  returns logical true."
  [pred coll]
  (lazy-seq
    (when-let [s (seq coll)]
      (if (chunked-seq? s)
        (let [c (chunk-first s)
              size (count c)
              b (chunk-buffer size)]
          (loop [i 0]
            (if (< i size)
              (do (when (pred (nth c i))
                    (chunk-append b (nth c i)))
                  (recur (inc i)))
              (chunk-cons (chunk b) (filter pred (chunk-rest s))))))
        (let [f (first s) r (rest s)]
          (if (pred f)
            (cons f (filter pred r))
            (filter pred r)))))))

(defn remove
  "Returns a lazy sequence of the items in coll for which (pred item)
  returns logical false."
  [pred coll] (filter (complement pred) coll))

(defn keep
  "Returns a lazy sequence of the non-nil results of (f item). Note:
  this means false return values will be included."
  [f coll]
  (lazy-seq
    (when-let [s (seq coll)]
      (let [x (f (first s))]
        (if (nil? x)
          (keep f (rest s))
          (cons x (keep f (rest s))))))))

;; --- Slicing (vanilla 2887-2980) ---

(defn take
  "Returns a lazy sequence of the first n items in coll, or all items
  if there are fewer than n."
  [n coll]
  (lazy-seq
    (when (pos? n)
      (when-let [s (seq coll)]
        (cons (first s) (take (dec n) (rest s)))))))

(defn take-while
  "Returns a lazy sequence of successive items from coll while (pred
  item) returns logical true."
  [pred coll]
  (lazy-seq
    (when-let [s (seq coll)]
      (when (pred (first s))
        (cons (first s) (take-while pred (rest s)))))))

(defn drop
  "Returns a lazy sequence of all but the first n items in coll."
  [n coll]
  (let [step (fn [n coll]
               (let [s (seq coll)]
                 (if (and (pos? n) s)
                   (recur (dec n) (rest s))
                   s)))]
    (lazy-seq (step n coll))))

(defn drop-while
  "Returns a lazy sequence of the items in coll starting from the
  first item for which (pred item) returns logical false."
  [pred coll]
  (let [step (fn [pred coll]
               (let [s (seq coll)]
                 (if (and s (pred (first s)))
                   (recur pred (rest s))
                   s)))]
    (lazy-seq (step pred coll))))

;; --- Generators (vanilla ~2995-3100) ---

(defn iterate
  "Returns a lazy sequence of x, (f x), (f (f x)) etc. f must be
  free of side-effects."
  [f x] (lazy-seq (cons x (iterate f (f x)))))

(defn repeat
  "Returns a lazy (infinite!, or length n if supplied) sequence of xs."
  ([x] (lazy-seq (cons x (repeat x))))
  ([n x] (take n (repeat x))))

(defn range
  "Returns a lazy seq of nums from start (inclusive) to end (exclusive),
  by step, where start defaults to 0 and step to 1. When end is not
  supplied, returns an infinite sequence. When step is equal to 0,
  returns an infinite sequence of start."
  ([] (iterate inc 0))
  ([end] (if (pos? end) (range 0 end 1) ()))
  ([start end] (range start end 1))
  ([start end step]
   (lazy-seq
     (if (or (and (pos? step) (< start end))
             (and (neg? step) (> start end))
             (and (zero? step) (not= start end)))
       (cons start (range (+ start step) end step))
       ()))))

(defn cycle
  "Returns a lazy (infinite!) sequence of repetitions of the items in coll."
  [coll]
  (lazy-seq
    (when-let [s (seq coll)]
      (concat s (cycle s)))))

(defn repeatedly
  "Takes a function of no args, presumably with side effects, and
  returns an infinite (or length n if supplied) lazy seq of calls
  to it."
  ([f] (lazy-seq (cons (f) (repeatedly f))))
  ([n f] (take n (repeatedly f))))

;; --- Concat helpers (vanilla ~2820, 3070) ---

(defn mapcat
  "Returns the result of applying concat to the result of applying map
  to f and colls."
  ([f coll] (apply concat (map f coll)))
  ([f c1 c2] (apply concat (map f c1 c2)))
  ([f c1 c2 c3] (apply concat (map f c1 c2 c3))))

(defn interleave
  "Returns a lazy seq of the first item in each coll, then the second etc."
  ([] ())
  ([c1] (lazy-seq c1))
  ([c1 c2]
   (lazy-seq
     (let [s1 (seq c1) s2 (seq c2)]
       (when (and s1 s2)
         (cons (first s1)
               (cons (first s2)
                     (interleave (rest s1) (rest s2)))))))))

(defn interpose
  "Returns a lazy seq of the elements of coll separated by sep."
  [sep coll]
  (drop 1 (interleave (repeat sep) coll)))

;; --- Sort + comparator (vanilla ~3170) ---

(defn comparator
  "Returns a Java Comparator-compatible fn from a Clojure predicate — where
  `(pred x y)` is true means x < y. Numeric 3-way comparators (returning
  -1/0/1) are usable directly by sort/sort-by without going through this."
  [pred]
  (fn [x y]
    (cond (pred x y) -1 (pred y x) 1 :else 0)))

(defn sort
  "Returns a sorted sequence of the items in coll. If no comparator is
  supplied, uses `compare`."
  ([coll] (sort compare coll))
  ([cmp coll]
   (if (seq coll)
     (clojure.lang.RT/sort-with cmp coll)
     ())))

(defn sort-by
  "Returns a sorted sequence of the items in coll, where the sort order is
  determined by comparing (keyfn item)."
  ([keyfn coll] (sort-by keyfn compare coll))
  ([keyfn cmp coll]
   (sort (fn [x y] (cmp (keyfn x) (keyfn y))) coll)))

;;;;;;;;;;;;;;;;;;;;;; Transducers (vanilla ~6900-7100) ;;;;;;;;;;;;;;;;;;;;;;

;; `completing` and `transduce` are the bridge between transducers and
;; reducers. `transduce` applies a transducer to a reducer, reduces over a
;; collection, and calls the completion arity at the end.

(defn completing
  "Takes a reducing function f of 2 args and returns a fn suitable for
  transduce by adding an arity-1 signature that calls cf (default
  identity) on the result argument."
  ([f] (completing f identity))
  ([f cf]
   (fn
     ([] (f))
     ([x] (cf x))
     ([x y] (f x y)))))

(defn transduce
  "reduce with a transformation of f (xform). If init is not supplied,
  (f) will be called to produce it. f should be a reducing step function
  that accepts both 1 and 2 arguments (see `completing`)."
  ([xform f coll] (transduce xform f (f) coll))
  ([xform f init coll]
   (let [rf (xform f)
         ret (reduce rf init coll)]
     (rf (unreduced ret)))))

;; --- Re-port map/filter/remove/keep with transducer arities ---
;; These REPLACE the earlier defns. Clojure vars get re-bound to the new
;; versions; previous code that captured the old fn via var lookup picks up
;; the new behavior on subsequent calls.

(defn map
  "Returns a lazy sequence consisting of the result of applying f to each
  element of coll. Returns a transducer when no coll is provided."
  ([f]
   (fn [rf]
     (fn
       ([] (rf))
       ([result] (rf result))
       ([result input] (rf result (f input)))
       ([result input & inputs] (rf result (apply f input inputs))))))
  ([f coll]
   (lazy-seq
     (when-let [s (seq coll)]
       (if (chunked-seq? s)
         (let [c (chunk-first s)
               size (count c)
               b (chunk-buffer size)]
           (loop [i 0]
             (if (< i size)
               (do (chunk-append b (f (nth c i)))
                   (recur (inc i)))
               (chunk-cons (chunk b) (map f (chunk-rest s))))))
         (cons (f (first s)) (map f (rest s)))))))
  ([f c1 c2]
   (lazy-seq
     (let [s1 (seq c1) s2 (seq c2)]
       (when (and s1 s2)
         (cons (f (first s1) (first s2))
               (map f (rest s1) (rest s2)))))))
  ([f c1 c2 c3]
   (lazy-seq
     (let [s1 (seq c1) s2 (seq c2) s3 (seq c3)]
       (when (and s1 s2 s3)
         (cons (f (first s1) (first s2) (first s3))
               (map f (rest s1) (rest s2) (rest s3))))))))

(defn filter
  "Returns a lazy sequence of items in coll for which (pred item) is truthy.
  Returns a transducer when no coll is provided."
  ([pred]
   (fn [rf]
     (fn
       ([] (rf))
       ([result] (rf result))
       ([result input] (if (pred input) (rf result input) result)))))
  ([pred coll]
   (lazy-seq
     (when-let [s (seq coll)]
       (if (chunked-seq? s)
         (let [c (chunk-first s)
               size (count c)
               b (chunk-buffer size)]
           (loop [i 0]
             (if (< i size)
               (do (when (pred (nth c i))
                     (chunk-append b (nth c i)))
                   (recur (inc i)))
               (chunk-cons (chunk b) (filter pred (chunk-rest s))))))
         (let [f (first s) r (rest s)]
           (if (pred f)
             (cons f (filter pred r))
             (filter pred r))))))))

(defn remove
  "Returns a lazy sequence of items in coll for which (pred item) is falsey.
  Returns a transducer when no coll is provided."
  ([pred] (filter (complement pred)))
  ([pred coll] (filter (complement pred) coll)))

(defn keep
  "Returns a lazy sequence of the non-nil results of (f item).
  Returns a transducer when no coll is provided."
  ([f]
   (fn [rf]
     (fn
       ([] (rf))
       ([result] (rf result))
       ([result input]
        (let [v (f input)]
          (if (nil? v) result (rf result v)))))))
  ([f coll]
   (lazy-seq
     (when-let [s (seq coll)]
       (let [x (f (first s))]
         (if (nil? x)
           (keep f (rest s))
           (cons x (keep f (rest s)))))))))

;; --- Stateful transducers: take / drop / take-while / drop-while ---

(defn take
  "Returns a lazy sequence of the first n items in coll, or all if <n.
  Returns a stateful transducer when no coll is provided."
  ([n]
   (fn [rf]
     (let [nv (volatile! n)]
       (fn
         ([] (rf))
         ([result] (rf result))
         ([result input]
          (let [n @nv
                nn (vswap! nv dec)
                result (if (pos? n) (rf result input) result)]
            (if (not (pos? nn))
              (ensure-reduced result)
              result)))))))
  ([n coll]
   (lazy-seq
     (when (pos? n)
       (when-let [s (seq coll)]
         (cons (first s) (take (dec n) (rest s))))))))

(defn drop
  "Returns a lazy sequence of all but the first n items in coll.
  Returns a stateful transducer when no coll is provided."
  ([n]
   (fn [rf]
     (let [nv (volatile! n)]
       (fn
         ([] (rf))
         ([result] (rf result))
         ([result input]
          (let [n @nv]
            (vswap! nv dec)
            (if (pos? n) result (rf result input))))))))
  ([n coll]
   (let [step (fn [n coll]
                (let [s (seq coll)]
                  (if (and (pos? n) s)
                    (recur (dec n) (rest s))
                    s)))]
     (lazy-seq (step n coll)))))

(defn take-while
  "Returns a lazy sequence of successive items from coll while (pred item) is truthy.
  Returns a transducer when no coll is provided."
  ([pred]
   (fn [rf]
     (fn
       ([] (rf))
       ([result] (rf result))
       ([result input]
        (if (pred input) (rf result input) (reduced result))))))
  ([pred coll]
   (lazy-seq
     (when-let [s (seq coll)]
       (when (pred (first s))
         (cons (first s) (take-while pred (rest s))))))))

(defn drop-while
  "Returns a lazy sequence of items from coll starting where (pred item) is falsey.
  Returns a stateful transducer when no coll is provided."
  ([pred]
   (fn [rf]
     (let [dv (volatile! true)]
       (fn
         ([] (rf))
         ([result] (rf result))
         ([result input]
          (let [drop? @dv]
            (if (and drop? (pred input))
              result
              (do (vreset! dv nil) (rf result input)))))))))
  ([pred coll]
   (let [step (fn [pred coll]
                (let [s (seq coll)]
                  (if (and s (pred (first s)))
                    (recur pred (rest s))
                    s)))]
     (lazy-seq (step pred coll)))))

;; --- cat, mapcat, interpose, dedupe ---

(defn ^:private preserving-reduced
  [rf]
  (fn [a b]
    (let [ret (rf a b)]
      (if (reduced? ret) (reduced ret) ret))))

(def cat
  "Transducer that concatenates the contents of each input, which must
  be a collection, into the reduction."
  (fn [rf]
    (let [rrf (preserving-reduced rf)]
      (fn
        ([] (rf))
        ([result] (rf result))
        ([result input] (reduce rrf result input))))))

(defn mapcat
  "Returns the result of applying concat to the result of applying map
  to f and colls. Thus f should return a collection. Returns a
  transducer when no colls are provided."
  ([f] (comp (map f) cat))
  ([f coll] (apply concat (map f coll)))
  ([f c1 c2] (apply concat (map f c1 c2)))
  ([f c1 c2 c3] (apply concat (map f c1 c2 c3))))

(defn interpose
  "Returns a lazy seq of the elements of coll separated by sep.
  Returns a stateful transducer when no coll is provided."
  ([sep]
   (fn [rf]
     (let [started (volatile! false)]
       (fn
         ([] (rf))
         ([result] (rf result))
         ([result input]
          (if @started
            (let [sresult (rf result sep)]
              (if (reduced? sresult)
                sresult
                (rf sresult input)))
            (do (vreset! started true)
                (rf result input))))))))
  ([sep coll] (drop 1 (interleave (repeat sep) coll))))

;; --- sequence (hoisted above dedupe since dedupe's coll-arity uses it) ---
;; Simplified: eagerly materialize via transduce into a vector's seq.
;; Vanilla's sequence is lazy; this Phase-A port is strict.

(defn sequence
  "Coerces `coll` to a (possibly empty) sequence. Unlike `seq`, returns an
  empty list rather than nil when `coll` is empty or nil — so
  `(if (sequence coll) …)` is always truthy. With transducer form: returns
  a seq of applying xform to coll. Multi-coll form walks colls in parallel
  and invokes the xform's step fn with one input per coll."
  ([coll] (or (seq coll) '()))
  ([xform coll]
   (or (seq (transduce xform (completing conj) [] coll)) '()))
  ([xform coll & colls]
   (let [rf (xform (completing conj))]
     (loop [acc []
            srcs (cons coll colls)]
       (let [seqs (map seq srcs)]
         (if (every? identity seqs)
           (let [result (apply rf acc (map first seqs))]
             (if (reduced? result)
               (or (seq (rf (deref result))) '())
               (recur result (map rest srcs))))
           (or (seq (rf acc)) '())))))))

(defn dedupe
  "Returns a lazy sequence removing consecutive duplicates in coll.
  Returns a transducer when no coll is provided."
  ([]
   (fn [rf]
     (let [pv (volatile! ::none)]
       (fn
         ([] (rf))
         ([result] (rf result))
         ([result input]
          (let [prior @pv]
            (vreset! pv input)
            (if (= prior input)
              result
              (rf result input))))))))
  ([coll] (sequence (dedupe) coll)))

;; --- into (transducer-capable) ---

(defn into
  "Returns a new coll consisting of to-coll with all of the items of
  from-coll conjoined. A transducer may be supplied."
  ([] [])
  ([to] to)
  ([to from] (reduce conj to from))
  ([to xform from] (transduce xform (completing conj) to from)))

;;;;;;;;;;;;;;;;;;;;;; Slicing extensions + aggregation ;;;;;;;;;;;;;;;;;;;;;;

;; --- Slicing extensions (vanilla ~2890-3070) ---

(defn split-at
  "Returns a vector of [(take n coll) (drop n coll)]."
  [n coll] [(take n coll) (drop n coll)])

(defn split-with
  "Returns a vector of [(take-while pred coll) (drop-while pred coll)]."
  [pred coll] [(take-while pred coll) (drop-while pred coll)])

(defn take-last
  "Returns a seq of the last n items in coll. Depending on the type of
  coll may be no better than linear time."
  [n coll]
  (loop [s (seq coll) lead (seq (drop n coll))]
    (if lead
      (recur (next s) (next lead))
      s)))

(defn drop-last
  "Return a lazy sequence of all but the last n (default 1) items in coll."
  ([s] (drop-last 1 s))
  ([n s] (map (fn [x _] x) s (drop n s))))

(defn take-nth
  "Returns a lazy seq of every nth item in coll. Returns a stateful
  transducer when no collection is provided."
  ([n]
   (fn [rf]
     (let [iv (volatile! -1)]
       (fn
         ([] (rf))
         ([result] (rf result))
         ([result input]
          (let [i (vswap! iv inc)]
            (if (zero? (rem i n))
              (rf result input)
              result)))))))
  ([n coll]
   (lazy-seq
     (when-let [s (seq coll)]
       (cons (first s) (take-nth n (drop n s)))))))

(defn nthnext
  "Returns the nth next of coll, (seq coll) when n is 0."
  [coll n]
  (loop [n n xs (seq coll)]
    (if (and xs (pos? n))
      (recur (dec n) (next xs))
      xs)))

(defn nthrest
  "Returns the nth rest of coll, coll when n is 0."
  [coll n]
  (loop [n n xs coll]
    (if (and (pos? n) (seq xs))
      (recur (dec n) (rest xs))
      xs)))

;; --- Map aggregation (vanilla ~3038-3080) ---

(defn merge
  "Returns a map that consists of the rest of the maps conj-ed onto the
  first. If a key occurs in more than one map, the mapping from the
  latter (left-to-right) will be the mapping in the result."
  [& maps]
  (when (some identity maps)
    (reduce #(conj (or %1 {}) %2) maps)))

(defn merge-with
  "Returns a map that consists of the rest of the maps conj-ed onto the
  first. If a key occurs in more than one map, the mapping(s) from the
  latter (left-to-right) will be combined with the mapping in the
  result by calling (f val-in-result val-in-latter)."
  [f & maps]
  (when (some identity maps)
    (let [merge-entry (fn [m e]
                        (let [k (key e) v (val e)]
                          (if (contains? m k)
                            (assoc m k (f (get m k) v))
                            (assoc m k v))))
          merge-map (fn [m1 m2] (reduce merge-entry (or m1 {}) (seq m2)))]
      (reduce merge-map maps))))

(defn zipmap
  "Returns a map with the keys mapped to the corresponding vals."
  [keys vals]
  (loop [map {} ks (seq keys) vs (seq vals)]
    (if (and ks vs)
      (recur (assoc map (first ks) (first vs)) (next ks) (next vs))
      map)))

;; --- Grouping (vanilla ~7400) ---

(defn group-by
  "Returns a map of the elements of coll keyed by the result of f on
  each element."
  [f coll]
  (reduce
    (fn [ret x]
      (let [k (f x)]
        (assoc ret k (conj (get ret k []) x))))
    {}
    coll))

(defn frequencies
  "Returns a map from distinct items in coll to the number of times they appear."
  [coll]
  (reduce (fn [counts x] (assoc counts x (inc (get counts x 0))))
          {}
          coll))

(defn reductions
  "Returns a lazy seq of the intermediate values of the reduction (as
  per reduce) of coll by f, starting with init."
  ([f coll]
   (lazy-seq
     (if-let [s (seq coll)]
       (reductions f (first s) (rest s))
       (list (f)))))
  ([f init coll]
   (cons init
         (lazy-seq
           (when-let [s (seq coll)]
             (reductions f (f init (first s)) (rest s)))))))

;; --- Predicate combinators (vanilla ~7430) ---

(defn every-pred
  "Takes a set of predicates and returns a function f that returns true
  if all of its composing predicates return a logical true value against
  all of its arguments, else returns false."
  ([p]
   (fn ep1
     ([] true)
     ([x] (boolean (p x)))
     ([x y] (boolean (and (p x) (p y))))
     ([x y z] (boolean (and (p x) (p y) (p z))))
     ([x y z & args] (boolean (and (ep1 x y z)
                                   (every? p args))))))
  ([p1 p2]
   (fn ep2
     ([] true)
     ([x] (boolean (and (p1 x) (p2 x))))
     ([x y] (boolean (and (p1 x) (p1 y) (p2 x) (p2 y))))
     ([x y z] (boolean (and (p1 x) (p1 y) (p1 z) (p2 x) (p2 y) (p2 z))))
     ([x y z & args] (boolean (and (ep2 x y z)
                                   (every? (fn [%] (and (p1 %) (p2 %))) args))))))
  ([p1 p2 p3]
   (fn ep3
     ([] true)
     ([x] (boolean (and (p1 x) (p2 x) (p3 x))))
     ([x y] (boolean (and (p1 x) (p1 y) (p2 x) (p2 y) (p3 x) (p3 y))))
     ([x y z] (boolean (and (p1 x) (p1 y) (p1 z) (p2 x) (p2 y) (p2 z) (p3 x) (p3 y) (p3 z))))
     ([x y z & args] (boolean (and (ep3 x y z)
                                   (every? (fn [%] (and (p1 %) (p2 %) (p3 %))) args))))))
  ([p1 p2 p3 & ps]
   (let [ps (list* p1 p2 p3 ps)]
     (fn epn
       ([] true)
       ([x] (every? (fn [p] (p x)) ps))
       ([x y] (every? (fn [p] (and (p x) (p y))) ps))
       ([x y z] (every? (fn [p] (and (p x) (p y) (p z))) ps))
       ([x y z & args] (boolean (and (epn x y z)
                                     (every? (fn [p] (every? p args)) ps))))))))

(defn some-fn
  "Takes a set of predicates and returns a function f that returns the
  first logical true value returned by one of its composing predicates
  against any of its arguments, else returns false."
  ([p]
   (fn sp1
     ([] nil)
     ([x] (p x))
     ([x y] (or (p x) (p y)))
     ([x y z] (or (p x) (p y) (p z)))
     ([x y z & args] (or (sp1 x y z)
                         (some p args)))))
  ([p1 p2]
   (fn sp2
     ([] nil)
     ([x] (or (p1 x) (p2 x)))
     ([x y] (or (p1 x) (p1 y) (p2 x) (p2 y)))
     ([x y z] (or (p1 x) (p1 y) (p1 z) (p2 x) (p2 y) (p2 z)))
     ([x y z & args] (or (sp2 x y z)
                         (some (fn [%] (or (p1 %) (p2 %))) args)))))
  ([p1 p2 p3]
   (fn sp3
     ([] nil)
     ([x] (or (p1 x) (p2 x) (p3 x)))
     ([x y] (or (p1 x) (p1 y) (p2 x) (p2 y) (p3 x) (p3 y)))
     ([x y z] (or (p1 x) (p1 y) (p1 z) (p2 x) (p2 y) (p2 z) (p3 x) (p3 y) (p3 z)))
     ([x y z & args] (or (sp3 x y z)
                         (some (fn [%] (or (p1 %) (p2 %) (p3 %))) args)))))
  ([p1 p2 p3 & ps]
   (let [ps (list* p1 p2 p3 ps)]
     (fn spn
       ([] nil)
       ([x] (some (fn [p] (p x)) ps))
       ([x y] (some (fn [p] (or (p x) (p y))) ps))
       ([x y z] (some (fn [p] (or (p x) (p y) (p z))) ps))
       ([x y z & args] (or (spn x y z)
                           (some (fn [p] (some p args)) ps)))))))

;; --- sequential? + tree-seq + flatten (vanilla ~3900, ~5000, ~7340) ---

(defn sequential?
  "Returns true if coll implements Sequential."
  [coll] (clojure.lang.RT/instance-sequential? coll))

(defn tree-seq
  "Returns a lazy sequence of the nodes in a tree, via a depth-first walk.
  branch? must be a fn of one arg that returns true if passed a node that
  can have children (but may not). children must be a fn of one arg that
  returns a sequence of the child nodes."
  [branch? children root]
  (let [walk (fn walk [node]
               (lazy-seq
                 (cons node
                       (when (branch? node)
                         (mapcat walk (children node))))))]
    (walk root)))

(defn flatten
  "Takes any nested combination of sequential things (lists, vectors,
  etc.) and returns their contents as a single, flat lazy sequence."
  [x]
  (filter (complement sequential?)
          (rest (tree-seq sequential? seq x))))

;;;;;;;;;;;;;;;;;;;;;; Partition family + forcing + utility macros ;;;;;;;;;;;;;;;;;;;;;;

;; --- ifn?/fn? + forcing hoisted above partition family, since partition-all
;;     uses doall and trampoline uses fn? ---

(defn ifn?
  "Returns true if x implements IFn. Note that many data structures
  (e.g. sets, maps) implement IFn."
  [x] (clojure.lang.RT/instance-ifn? x))

(defn fn?
  "Returns true if x is a Fn, created by fn or defn — distinct from `ifn?`
  which also returns true for keywords, maps, sets, Vars, etc."
  [x] (clojure.lang.RT/instance-fn? x))

(defn dorun
  "Walks through a lazy sequence to force side-effects; returns nil."
  ([coll]
   (when-let [s (seq coll)]
     (recur (next s))))
  ([n coll]
   (when (and (seq coll) (pos? n))
     (recur (dec n) (next coll)))))

(defn doall
  "Like dorun but retains the head and returns it."
  ([coll] (dorun coll) coll)
  ([n coll] (dorun n coll) coll))

;; --- Partition family (vanilla ~3160-3250) ---

(defn partition
  "Returns a lazy sequence of lists of n items each, at offsets step
  apart. If step is not supplied, defaults to n, i.e. the partitions
  do not overlap. If a pad collection is supplied, use its elements as
  necessary to complete last partition upto n items. In case there
  are not enough padding elements, return a partition with less than
  n items."
  ([n coll] (partition n n coll))
  ([n step coll]
   (lazy-seq
     (when-let [s (seq coll)]
       (let [p (take n s)]
         (when (= n (count p))
           (cons p (partition n step (nthrest s step))))))))
  ([n step pad coll]
   (lazy-seq
     (when-let [s (seq coll)]
       (let [p (take n s)]
         (if (= n (count p))
           (cons p (partition n step pad (nthrest s step)))
           (list (take n (concat p pad)))))))))

(defn partition-all
  "Returns a lazy sequence of lists like partition, but may include
  partitions with fewer than n items at the end. Returns a stateful
  transducer when no collection is provided."
  ([n]
   (fn [rf]
     (let [a (volatile! [])]
       (fn
         ([] (rf))
         ([result]
          (let [result (if (zero? (count @a))
                         result
                         (let [v @a]
                           (vreset! a [])
                           (unreduced (rf result v))))]
            (rf result)))
         ([result input]
          (vswap! a conj input)
          (if (= n (count @a))
            (let [v @a]
              (vreset! a [])
              (rf result v))
            result))))))
  ([n coll] (partition-all n n coll))
  ([n step coll]
   (lazy-seq
     (when-let [s (seq coll)]
       (let [seg (doall (take n s))]
         (cons seg (partition-all n step (nthrest s step))))))))

(defn partition-by
  "Applies f to each value in coll, splitting it each time f returns a
  new value. Returns a lazy seq of partitions. Returns a stateful
  transducer when no collection is provided."
  ([f]
   (fn [rf]
     (let [a (volatile! [])
           pv (volatile! ::none)]
       (fn
         ([] (rf))
         ([result]
          (let [result (if (zero? (count @a))
                         result
                         (let [v @a]
                           (vreset! a [])
                           (unreduced (rf result v))))]
            (rf result)))
         ([result input]
          (let [pval @pv
                val (f input)]
            (vreset! pv val)
            (if (or (identical? pval ::none)
                    (= val pval))
              (do (vswap! a conj input) result)
              (let [v @a]
                (vreset! a [])
                (let [ret (rf result v)]
                  (when-not (reduced? ret)
                    (vswap! a conj input))
                  ret)))))))))
  ([f coll]
   (lazy-seq
     (when-let [s (seq coll)]
       (let [fst (first s)
             fv (f fst)
             run (cons fst (take-while (fn [x] (= fv (f x))) (next s)))]
         (cons run (partition-by f (lazy-seq (drop (count run) s)))))))))

;; `doseq` — full grammar: multiple binding pairs, destructuring, and
;; :let/:when/:while modifiers. Chunked-seq fast path mirrors vanilla.
(defmacro doseq
  "Repeatedly executes body (presumably for side-effects) with
  bindings and filtering as provided by \"for\".  Does not retain
  the head of the sequence. Returns nil."
  [seq-exprs & body]
  (assert-args
    (vector? seq-exprs) "doseq requires a vector for its binding"
    (even? (count seq-exprs)) "doseq requires an even number of forms in binding vector")
  (let [step (fn step [recform exprs]
               (if (not exprs)
                 [true `(do ~@body)]
                 (let [k (first exprs)
                       v (second exprs)]
                   (if (keyword? k)
                     (let [steppair (step recform (nnext exprs))
                           needrec (steppair 0)
                           subform (steppair 1)]
                       (cond
                         (= k :let) [needrec `(let [~@v] ~subform)]
                         (= k :while) [false `(when ~v
                                                ~subform
                                                ~@(when needrec [recform]))]
                         (= k :when) [false `(if ~v
                                               (do
                                                 ~subform
                                                 ~@(when needrec [recform]))
                                               ~recform)]))
                     (let [seq- (gensym "seq_")
                           chunk- (gensym "chunk_")
                           count- (gensym "count_")
                           i- (gensym "i_")
                           recform `(recur (next ~seq-) nil 0 0)
                           steppair (step recform (nnext exprs))
                           needrec (steppair 0)
                           subform (steppair 1)
                           recform-chunk `(recur ~seq- ~chunk- ~count- (inc ~i-))
                           steppair-chunk (step recform-chunk (nnext exprs))
                           subform-chunk (steppair-chunk 1)]
                       [true
                        `(loop [~seq- (seq ~v), ~chunk- nil,
                                ~count- 0, ~i- 0]
                           (if (< ~i- ~count-)
                             (let [~k (nth ~chunk- ~i-)]
                               ~subform-chunk
                               ~@(when needrec [recform-chunk]))
                             (when-let [~seq- (seq ~seq-)]
                               (if (chunked-seq? ~seq-)
                                 (let [c# (chunk-first ~seq-)]
                                   (recur (chunk-rest ~seq-) c#
                                          (count c#) 0))
                                 (let [~k (first ~seq-)]
                                   ~subform
                                   ~@(when needrec [recform]))))))])))))]
    (nth (step nil (seq seq-exprs)) 1)))

;; `for` — list comprehension (vanilla core.clj ~4677). Yields a lazy
;; seq; supports :let / :when / :while modifiers; chunked-seq fast path
;; for the innermost binding. `.nth` interop is replaced with `nth`,
;; `unchecked-inc` with `inc`, and JVM `(int ...)` wrappers dropped.
(defmacro for
  "List comprehension. Takes a vector of one or more
   binding-form/collection-expr pairs, each followed by zero or more
   modifiers, and yields a lazy sequence of evaluations of expr.
   Collections are iterated in a nested fashion, rightmost fastest,
   and nested coll-exprs can refer to bindings created in prior
   binding-forms.  Supported modifiers are: :let [binding-form expr ...],
   :while test, :when test."
  [seq-exprs body-expr]
  (assert-args
    (vector? seq-exprs) "for requires a vector for its binding"
    (even? (count seq-exprs)) "for requires an even number of forms in binding vector")
  (let [to-groups (fn [seq-exprs]
                    (reduce (fn [groups kv]
                              (let [k (first kv) v (second kv)]
                                (if (keyword? k)
                                  (conj (pop groups) (conj (peek groups) [k v]))
                                  (conj groups [k v]))))
                            [] (partition 2 seq-exprs)))
        err (fn [& msg] (clojure.lang.RT/throw-iae (apply str msg)))
        emit-bind (fn emit-bind [groups]
                    (let [first-group (first groups)
                          bind (first first-group)
                          expr (second first-group)
                          mod-pairs (nnext first-group)
                          next-groups (next groups)
                          next-expr (second (first next-groups))
                          giter (gensym "iter__")
                          gxs (gensym "s__")
                          do-mod (fn do-mod [mods]
                                   (if (seq mods)
                                     (let [pair (first mods)
                                           k (first pair) v (second pair)
                                           etc (next mods)]
                                       (cond
                                         (= k :let) `(let ~v ~(do-mod etc))
                                         (= k :while) `(when ~v ~(do-mod etc))
                                         (= k :when) `(if ~v
                                                        ~(do-mod etc)
                                                        (recur (rest ~gxs)))
                                         (keyword? k) (err "Invalid 'for' keyword " k)
                                         :else (err "unreachable")))
                                     (if next-groups
                                       `(let [iterys# ~(emit-bind next-groups)
                                              fs# (seq (iterys# ~next-expr))]
                                          (if fs#
                                            (concat fs# (~giter (rest ~gxs)))
                                            (recur (rest ~gxs))))
                                       `(cons ~body-expr
                                              (~giter (rest ~gxs))))))]
                      (if next-groups
                        `(fn ~giter [~gxs]
                           (lazy-seq
                             (loop [~gxs ~gxs]
                               (when-first [~bind ~gxs]
                                 ~(do-mod mod-pairs)))))
                        (let [gi (gensym "i__")
                              gb (gensym "b__")
                              do-cmod (fn do-cmod [mods]
                                        (if (seq mods)
                                          (let [pair (first mods)
                                                k (first pair) v (second pair)
                                                etc (next mods)]
                                            (cond
                                              (= k :let) `(let ~v ~(do-cmod etc))
                                              (= k :while) `(when ~v ~(do-cmod etc))
                                              (= k :when) `(if ~v
                                                             ~(do-cmod etc)
                                                             (recur (inc ~gi)))
                                              (keyword? k) (err "Invalid 'for' keyword " k)
                                              :else (err "unreachable")))
                                          `(do (chunk-append ~gb ~body-expr)
                                               (recur (inc ~gi)))))]
                          `(fn ~giter [~gxs]
                             (lazy-seq
                               (loop [~gxs ~gxs]
                                 (when-let [~gxs (seq ~gxs)]
                                   (if (chunked-seq? ~gxs)
                                     (let [c# (chunk-first ~gxs)
                                           size# (count c#)
                                           ~gb (chunk-buffer size#)]
                                       (if (loop [~gi 0]
                                             (if (< ~gi size#)
                                               (let [~bind (nth c# ~gi)]
                                                 ~(do-cmod mod-pairs))
                                               true))
                                         (chunk-cons
                                           (chunk ~gb)
                                           (~giter (chunk-rest ~gxs)))
                                         (chunk-cons (chunk ~gb) nil)))
                                     (let [~bind (first ~gxs)]
                                       ~(do-mod mod-pairs)))))))))))]
    `(let [iter# ~(emit-bind (to-groups seq-exprs))]
       (iter# ~(second seq-exprs)))))

;; --- Utility macros + memoize/trampoline + condp (vanilla ~3800-4100) ---

(defmacro when-first
  "bindings => x xs
  Same as (when (seq xs) (let [x (first xs)] body))"
  [bindings & body]
  (assert-args
    (vector? bindings) "when-first requires a vector for its binding"
    (= 2 (count bindings)) "when-first requires exactly 2 forms in binding vector")
  (let [x (bindings 0) xs (bindings 1)]
    `(when (seq ~xs)
       (let [~x (first ~xs)]
         ~@body))))

(defmacro while
  "Repeatedly executes body while test expression is true. Presumes
  some side-effect will cause test to become false/nil. Returns nil."
  [test & body]
  `(loop []
     (when ~test
       ~@body
       (recur))))

(defn memoize
  "Returns a memoized version of a referentially transparent function.
  The memoized version of the function keeps a cache of the mapping
  from arguments to results and, when calls with the same arguments
  are repeated often, has higher performance at the expense of higher
  memory use."
  [f]
  (let [mem (atom {})]
    (fn [& args]
      (if-let [e (find @mem args)]
        (val e)
        (let [ret (apply f args)]
          (swap! mem assoc args ret)
          ret)))))

(defn trampoline
  "trampoline can be used to convert algorithms requiring mutual
  recursion without stack consumption. Calls f with supplied args, if
  any. If f returns a fn, calls that fn with no arguments, and
  continues to repeat, until the return value is not a fn, then
  returns that non-fn value."
  ([f]
   (let [ret (f)]
     (if (fn? ret)
       (recur ret)
       ret)))
  ([f & args] (trampoline (fn [] (apply f args)))))

(defmacro condp
  "Takes a binary predicate, an expression, and a set of clauses. Each
  clause can take the form of either:
    test-expr result-expr
    test-expr :>> result-fn
  For each clause, (pred test-expr expr) is evaluated. If it returns
  logical true, the clause is a match. If a binary clause matches, the
  result-expr is returned. If a ternary clause matches, its result-fn,
  which must be a unary function, is called with the result of the
  predicate as its argument, the result of that call being the return
  value of condp. A single default expression can follow the clauses,
  and its value will be returned if no clause matches."
  [pred expr & clauses]
  (let [gpred (gensym "pred__")
        gexpr (gensym "expr__")
        emit (fn emit [pred expr args]
               (let [take-n (if (= :>> (second args)) 3 2)
                     clause (take take-n args)
                     more (drop take-n args)
                     n (count clause)
                     a (first clause)
                     b (second clause)
                     c (first (drop 2 clause))]
                 (cond
                   (= 0 n) `(clojure.lang.RT/throw-iae (clojure.lang.RT/str-concat "No matching clause: " ~expr))
                   (= 1 n) a
                   (= 2 n) `(if (~pred ~a ~expr)
                              ~b
                              ~(emit pred expr more))
                   :else `(if-let [p# (~pred ~a ~expr)]
                            (~c p#)
                            ~(emit pred expr more)))))]
    `(let [~gpred ~pred
           ~gexpr ~expr]
       ~(emit gpred gexpr clauses))))

;;;;;;;;;;;;;;;;;;;;;; destructure + real let / loop / fn ;;;;;;;;;;;;;;;;;;;;;;

;; `destructure` expands a binding vector that may contain vector/map
;; destructuring patterns into a flat pairs-of-symbols form suitable for
;; `let*`. Called at macroexpansion time by the `let`, `loop`, and `fn`
;; macros below, which replace the bootstrap versions defined at the top of
;; this file.
;;
;; Supported patterns:
;;   sym                — plain binding
;;   [a b c]            — positional from vector/seq, nth lookups
;;   [a b & rest]       — rest bound to seq of remaining items
;;   [a :as whole]      — whole value also bound
;;   {:keys [a b]}      — {a :a, b :b}
;;   {:or {a 0}}        — defaults when key missing
;;   {:as whole}        — whole map bound
;;   nested             — any pattern position can be a nested pattern
;;
;; Simplifications vs vanilla: no :strs / :syms / namespaced-keyword
;; destructuring yet.
(defn destructure [bindings]
  (let [bents (partition 2 bindings)
        pb (fn pb [bvec b v]
             (let [pvec
                   (fn [bvec b val]
                     (let [gvec (gensym "vec__")
                           gseq (gensym "seq__")
                           has-rest (some (fn [x] (= x '&)) b)]
                       (loop [ret (let [ret0 (conj bvec gvec val)]
                                    (if has-rest
                                      (conj ret0 gseq (list 'clojure.core/seq gvec))
                                      ret0))
                              n 0
                              bs b
                              seen-rest? false]
                         (if (seq bs)
                           (let [firstb (first bs)]
                             (cond
                               (= firstb '&)
                               (let [rest-b (second bs)]
                                 ;; `(fn [& {:keys [x]}] …)` — kwargs-style
                                 ;; destructuring. The rest seq must be
                                 ;; coerced to a map before `{:keys …}` can
                                 ;; look up keys on it.
                                 (if (map? rest-b)
                                   (recur (pb ret rest-b
                                             (list 'clojure.core/apply
                                                   'clojure.core/hash-map
                                                   gseq))
                                          n (nnext bs) true)
                                   (recur (pb ret rest-b gseq)
                                          n (nnext bs) true)))

                               (= firstb :as)
                               (pb ret (second bs) gvec)

                               :else
                               (if seen-rest?
                                 (clojure.lang.RT/throw-iae
                                   "Unsupported binding form, only :as can follow & parameter")
                                 (recur
                                   (if has-rest
                                     ;; Consuming the head of gseq; also advance gseq.
                                     (let [ret2 (pb ret firstb (list 'clojure.core/first gseq))]
                                       (conj ret2 gseq (list 'clojure.core/next gseq)))
                                     ;; Positional nth into gvec.
                                     (pb ret firstb (list 'clojure.core/nth gvec n nil)))
                                   (inc n)
                                   (next bs)
                                   seen-rest?))))
                           ret))))
                   pmap
                   (fn [bvec b v]
                     ;; Normalize `{:keys [a b]}` / `{:strs [a b]}` /
                     ;; `{:syms [a b]}` / namespaced-keys (`{:ns/keys [a b]}`,
                     ;; `{:ns/syms [a b]}`) into explicit `{local key}`
                     ;; entries. Also handle a keyword-literal key INSIDE
                     ;; `:keys` (e.g. `{:keys [:a :b]}`) and the
                     ;; `a/b`-style namespaced symbol (key becomes `:a/b`,
                     ;; local becomes bare `b`).
                     (let [b (reduce
                               (fn [acc kw]
                                 (if-let [ks (get acc kw)]
                                   (let [acc (dissoc acc kw)
                                         kw-ns (namespace kw)
                                         kind  (name kw)]
                                     (reduce
                                       (fn [m entry]
                                         (let [entry-sym (cond
                                                           (keyword? entry) (symbol (namespace entry) (name entry))
                                                           :else entry)
                                               local (symbol (name entry-sym))
                                               ;; ns comes from :ns/keys if present,
                                               ;; else from the entry symbol itself.
                                               entry-ns (or kw-ns (namespace entry-sym))
                                               the-key (cond
                                                         (= kind "keys")
                                                         (if entry-ns
                                                           (keyword entry-ns (name entry-sym))
                                                           (keyword (name entry-sym)))
                                                         (= kind "syms")
                                                         (if entry-ns
                                                           (list 'quote (symbol entry-ns (name entry-sym)))
                                                           (list 'quote (symbol (name entry-sym))))
                                                         (= kind "strs")
                                                         (name entry-sym))]
                                           (assoc m local the-key)))
                                       acc
                                       ks))
                                   acc))
                               b
                               ;; Collect all keys-style keywords actually present
                               ;; in b (each is either :keys/:strs/:syms or an
                               ;; ns-qualified variant like :my.ns/keys).
                               (filter (fn [k]
                                         (and (keyword? k)
                                              (#{"keys" "strs" "syms"} (name k))))
                                       (keys b)))
                           gmap (gensym "map__")
                           defaults (:or b)
                           ;; Coerce a seq value to a map (vanilla behavior):
                           ;; `(let [{:as x} '()] x)` → `x = {}`.
                           ;; `(let [{:keys [a]} '(:a 1)] a)` → `a = 1`.
                           ;; Non-seqs pass through unchanged.
                           ret (conj bvec gmap v
                                     gmap (list 'if (list 'clojure.core/seq? gmap)
                                                (list 'if (list 'clojure.core/seq gmap)
                                                      (list 'clojure.core/apply
                                                            'clojure.core/hash-map gmap)
                                                      {})
                                                gmap))
                           ret (if (:as b) (conj ret (:as b) gmap) ret)
                           entries (dissoc b :as :or)]
                       (reduce (fn [ret entry]
                                 (let [bb (key entry)
                                       bk (val entry)
                                       bv (if (and defaults (contains? defaults bb))
                                            (list 'clojure.core/get gmap bk (defaults bb))
                                            (list 'clojure.core/get gmap bk))]
                                   (if (symbol? bb)
                                     (-> ret (conj bb) (conj bv))
                                     (pb ret bb bv))))
                               ret
                               entries)))]
               (cond
                 (symbol? b) (-> bvec (conj b) (conj v))
                 (vector? b) (pvec bvec b v)
                 (map? b)    (pmap bvec b v)
                 :else (clojure.lang.RT/throw-iae
                         (clojure.lang.RT/str-concat "Unsupported binding form: " b)))))
        process-entry (fn [bvec b] (pb bvec (first b) (second b)))]
    (if (every? symbol? (map first bents))
      bindings
      (reduce process-entry [] bents))))

;; --- Replace bootstrap let / loop / fn with destructure-aware versions ---
;;
;; Vanilla does the same swap around line 3700 of core.clj. Existing code
;; compiled against the bootstrap versions is unaffected (those calls still
;; splat to let*/loop*/fn* and the expansion is identical for plain-symbol
;; bindings); new code can now use full destructuring.

(defmacro let
  "Evaluates the exprs in a lexical context in which the symbols in
  the binding-forms are bound to their respective init-exprs or parts
  therein."
  [bindings & body]
  (assert-args
    (vector? bindings) "let requires a vector for its binding"
    (even? (count bindings)) "let requires an even number of forms in binding vector")
  `(let* ~(destructure bindings) ~@body))

;; `update` isn't ported yet; use assoc+get for accumulator updates.
;; The macros below avoid destructured fn params too — their own bodies
;; compile against the bootstrap `fn` (which only allows plain symbols).

(defmacro loop
  "Evaluates the exprs in a lexical context in which the symbols in
  the binding-forms are bound to their respective init-exprs or parts
  therein. Acts as a recur target."
  [bindings & body]
  (assert-args
    (vector? bindings) "loop requires a vector for its binding"
    (even? (count bindings)) "loop requires an even number of forms in binding vector")
  (let [db (destructure bindings)]
    (if (= db bindings)
      `(loop* ~bindings ~@body)
      ;; Destructuring happened — reshape to:
      ;;   (let [g1 v1 g2 v2 ...] (loop* [g1 g1 g2 g2 ...] (let [orig-pat1 g1 ...] body)))
      (let [pairs (partition 2 bindings)
            orig-syms (map first pairs)
            orig-vals (map second pairs)
            gs (map (fn [_] (gensym "loop-arg__")) pairs)]
        `(let ~(vec (interleave gs orig-vals))
           (loop* ~(vec (interleave gs gs))
             (let ~(vec (interleave orig-syms gs))
               ~@body)))))))

;; `maybe-destructured` rewrites a param vector containing non-symbol
;; binding forms into a plain-symbol vector + a trailing `let` that does
;; the destructure. Vanilla's version lives at core.clj line ~4545.
(defn ^:private maybe-destructured
  [params body]
  (if (every? symbol? params)
    (cons params body)
    (loop [params params
           new-params []
           lets []
           prev-amp? false]
      (if params
        (let [p (first params)]
          (if (symbol? p)
            (recur (next params)
                   (conj new-params p)
                   lets
                   (= p '&))
            (let [gparam (gensym "p__")
                  ;; `(fn [& {:keys [x]}] …)` — the rest seq must be
                  ;; coerced to a map before `{:keys …}` can destructure it.
                  val (if (and prev-amp? (map? p))
                        (list 'clojure.core/apply 'clojure.core/hash-map gparam)
                        gparam)]
              (recur (next params)
                     (conj new-params gparam)
                     (conj (conj lets p) val)
                     false))))
        (list new-params (cons 'clojure.core/let (cons lets body)))))))

;; --- assert (vanilla ~4480) + :pre/:post condition handling -----------------

(defmacro assert
  "Evaluates expr and throws an AssertionError if it does not evaluate to
  logical true."
  ([x]
   (list 'clojure.core/when-not x
         (list 'clojure.lang.RT/throw-assert
               (list 'clojure.core/str
                     "Assert failed: "
                     (list 'clojure.lang.RT/pr-str (list 'quote x))))))
  ([x message]
   (list 'clojure.core/when-not x
         (list 'clojure.lang.RT/throw-assert
               (list 'clojure.core/str
                     "Assert failed: "
                     message
                     "\n"
                     (list 'clojure.lang.RT/pr-str (list 'quote x)))))))

;; `process-conditions` — if the first form of `body` is a {:pre … :post …}
;; map, consume it and wrap the remaining body with asserts. `%` in :post
;; conditions refers to the (let-bound) return value of the body.
(defn ^:private process-conditions
  [body]
  (let [cmap (first body)]
    (if (and (map? cmap)
             (next body)
             (or (contains? cmap :pre) (contains? cmap :post)))
      (let [rest-body (next body)
            pre-conds (:pre cmap)
            post-conds (:post cmap)
            pct (symbol "%")   ;; bare `%` symbol — NOT (quote %).
            mk-assert (fn* [c] (list 'clojure.core/assert c))
            pre-asserts (map mk-assert pre-conds)
            post-wrapped (if (seq post-conds)
                           (list
                             (concat
                               (list 'clojure.core/let
                                     (vector pct (cons 'do rest-body)))
                               (map mk-assert post-conds)
                               (list pct)))
                           rest-body)]
        (concat pre-asserts post-wrapped))
      body)))

;; Redefine `fn` with destructuring + :pre/:post conditions.
(defmacro fn
  "params => positional-params*, or positional-params* & rest-param
  positional-param => binding-form
  rest-param => binding-form
  binding-form => name, or destructuring-form

  Defines a function. A body may optionally start with a map of
  preconditions and/or postconditions:
    {:pre  [pre-expr*]
     :post [post-expr*]}
  Each pre-expr is evaluated before the function body; each post-expr is
  evaluated after, with `%` bound to the return value. A failing expression
  throws AssertionError."
  [& sigs]
  (let [name (if (clojure.lang.RT/instance-symbol? (first sigs)) (first sigs) nil)
        sigs (if name (next sigs) sigs)
        sigs (if (vector? (first sigs))
               (list sigs)
               (if (seq? (first sigs))
                 sigs
                 (clojure.lang.RT/throw-iae
                   (if (seq sigs)
                     (clojure.lang.RT/str-concat
                       "Parameter declaration "
                       (clojure.lang.RT/str-concat (str (first sigs))
                                                   " should be a vector"))
                     "Parameter declaration missing"))))
        psig (fn* psig [sig]
               (if (seq? sig)
                 nil
                 (clojure.lang.RT/throw-iae
                   (clojure.lang.RT/str-concat "Invalid signature " (str sig))))
               (let [params (first sig)
                     body (next sig)]
                 (if (vector? params)
                   nil
                   (clojure.lang.RT/throw-iae
                     (clojure.lang.RT/str-concat "Parameter declaration " (str params))))
                 (maybe-destructured params (process-conditions body))))
        new-sigs (map psig sigs)]
    (if name
      (cons 'fn* (cons name new-sigs))
      (cons 'fn* new-sigs))))

;; --- letfn (vanilla 4438) ---------------------------------------------------
;;
;; Expands to the `letfn*` special form (compiler/emit.rs). Each binding
;; pair `(fname args & body)` becomes `fname (fn fname args & body)` —
;; the inner fn carries its own name so error messages are useful.

(defmacro letfn
  "fnspec ==> (fname [params*] body) or (fname ([params*] body) ([params2*] body) …)

  Takes a vector of function specs and a body, and generates a set of
  bindings of the function names to the corresponding functions. The
  bindings are mutually recursive: each function may refer to any other
  by name."
  [fnspecs & body]
  (cons 'letfn*
        (cons (vec (mapcat
                     (fn [spec]
                       [(first spec)
                        (cons 'clojure.core/fn spec)])
                     fnspecs))
              body)))

;; --- ex-info / ex-data / ex-cause / ex-message (vanilla 5300) ---------------

(defn ex-info
  "Create an instance of ExceptionInfo, a RuntimeException subclass that
  carries a map of additional data."
  ([msg map]       (clojure.lang.RT/ex-info-impl msg map nil))
  ([msg map cause] (clojure.lang.RT/ex-info-impl msg map cause)))

(defn ex-data
  "Returns exception data (a map) if ex is an ExceptionInfo (or anything
  with a `.data` attribute). Otherwise returns nil."
  [ex] (clojure.lang.RT/ex-data-impl ex))

(defn ex-cause
  "Returns the cause of ex if ex is an ExceptionInfo. Otherwise nil."
  [ex] (clojure.lang.RT/ex-cause-impl ex))

(defn ex-message
  "Returns the message attached to ex (the first ctor arg)."
  [ex] (clojure.lang.RT/ex-message-impl ex))

;; --- fnil (vanilla 6573) ----------------------------------------------------

(defn fnil
  "Takes a function f, and returns a function that calls f, replacing a
  nil first (second, third) argument with the supplied value x (y, z).
  Note that the function f can take any number of arguments, not just the
  one(s) being nil-patched."
  ([f x]
   (fn
     ([a]         (f (if (nil? a) x a)))
     ([a b]       (f (if (nil? a) x a) b))
     ([a b c]     (f (if (nil? a) x a) b c))
     ([a b c & ds] (apply f (if (nil? a) x a) b c ds))))
  ([f x y]
   (fn
     ([a b]       (f (if (nil? a) x a) (if (nil? b) y b)))
     ([a b c]     (f (if (nil? a) x a) (if (nil? b) y b) c))
     ([a b c & ds] (apply f (if (nil? a) x a) (if (nil? b) y b) c ds))))
  ([f x y z]
   (fn
     ([a b c]     (f (if (nil? a) x a) (if (nil? b) y b) (if (nil? c) z c)))
     ([a b c & ds] (apply f (if (nil? a) x a) (if (nil? b) y b) (if (nil? c) z c) ds)))))

;; --- Regex (vanilla 4500-4600, 7100) ----------------------------------------

(defn re-pattern
  "Returns an instance of java.util.regex.Pattern (here: Python's
  re.Pattern), for use, e.g. in re-matcher."
  [s] (clojure.lang.RT/re-pattern s))

(defn re-find
  "Returns the next regex match, if any, of string to pattern, using
  re.search(). Uses re.Match's .group(0) and .groups()."
  [re s] (clojure.lang.RT/re-find-impl re s))

(defn re-matches
  "Returns the match, if any, of string to pattern, using re.fullmatch()."
  [re s] (clojure.lang.RT/re-matches-impl re s))

(defn re-seq
  "Returns a (lazy) sequence of successive matches of pattern in string."
  [re s] (clojure.lang.RT/re-seq-impl re s))

;; --- Parse helpers + random-uuid (vanilla 7300, Clojure 1.11+) --------------

(defn parse-long
  "Parse string of decimal digits as a long. Returns nil if invalid."
  [s] (clojure.lang.RT/parse-long-impl s))

(defn parse-double
  "Parse string of floating-point literal as a double. Returns nil if invalid."
  [s] (clojure.lang.RT/parse-double-impl s))

(defn parse-boolean
  "Parse strings \"true\" or \"false\" and return a boolean. Returns nil if
  the string is neither."
  [s] (clojure.lang.RT/parse-boolean-impl s))

(defn parse-uuid
  "Parse a string representing a UUID. Returns nil if invalid."
  [s] (clojure.lang.RT/parse-uuid-impl s))

(defn random-uuid
  "Returns a pseudo-randomly generated UUID."
  [] (clojure.lang.RT/random-uuid-impl))

;; --- Missing collection / type predicates -----------------------------------

(defn coll?
  "Returns true if x implements IPersistentCollection."
  [x] (clojure.lang.RT/instance-coll? x))

(defn list?
  "Returns true if x implements IPersistentList."
  [x] (clojure.lang.RT/instance-list? x))

(defn counted?
  "Returns true if coll implements count in constant time."
  [x] (clojure.lang.RT/instance-counted? x))

(defn seqable?
  "Returns true if (seq x) will succeed, false otherwise."
  [x] (or (nil? x) (clojure.lang.RT/instance-seqable? x)))

(defn reversible?
  "Returns true if coll implements Reversible."
  [x] (clojure.lang.RT/instance-reversible? x))

(defn indexed?
  "Returns true if coll implements nth in constant time."
  [x] (clojure.lang.RT/instance-indexed? x))

(defn associative?
  "Returns true if coll implements Associative."
  [x] (clojure.lang.RT/instance-associative? x))

(defn empty?
  "Returns true if coll has no items — same as (not (seq coll))."
  [coll] (not (seq coll)))

(defn not-empty
  "If coll is empty, returns nil, else coll."
  [coll] (when (seq coll) coll))

(defn distinct?
  "Returns true if no two of the arguments are =."
  ([_] true)
  ([x y] (not (= x y)))
  ([x y & more]
   (if (not= x y)
     (loop [s #{x y} [a & ns] more]
       (if a
         (if (contains? s a)
           false
           (recur (conj s a) ns))
         true))
     false)))

(defn var?
  "Returns true if v is of type clojure.lang.Var."
  [v] (clojure.lang.RT/instance-var? v))

(defn special-symbol?
  "Returns true if s names one of the special forms."
  [s]
  (and (symbol? s)
       (nil? (namespace s))
       (contains? (hash-set "quote" "if" "do" "let*" "loop*" "recur" "fn*"
                            "def" "set!" "throw" "try" "var" "letfn*"
                            "monitor-enter" "monitor-exit" "." "catch"
                            "finally" "new")
                  (name s))))

(defn bound?
  "Returns true if all of the vars provided as arguments have any bound
  value, root or thread. Implies that deref'ing the var will succeed."
  [& vars]
  (loop [vs vars]
    (if (seq vs)
      (if (clojure.lang.RT/var-bound? (first vs))
        (recur (next vs))
        false)
      true)))

(defn thread-bound?
  "Returns true if all of the vars provided as arguments have thread-local
  bindings."
  [& vars]
  (loop [vs vars]
    (if (seq vs)
      (if (clojure.lang.RT/var-thread-bound? (first vs))
        (recur (next vs))
        false)
      true)))

(defn inst?
  "Returns true if x is a python datetime/date instance."
  [x] (clojure.lang.RT/inst-q x))

(defn inst-ms
  "Return the number of milliseconds since the epoch for the given inst."
  [inst] (clojure.lang.RT/inst-ms-impl inst))

(defn uuid?
  "Returns true if x is a uuid.UUID."
  [x] (clojure.lang.RT/uuid-q x))

(defn NaN?
  "Returns true if num is NaN, false otherwise."
  [n] (clojure.lang.RT/nan-q n))

(defn infinite?
  "Returns true if num is +/- infinity, false otherwise."
  [n] (clojure.lang.RT/infinite-q n))

;; --- Missing collection helpers --------------------------------------------

(defn empty
  "Returns an empty collection of the same category as coll, or nil."
  [coll]
  (cond
    (vector? coll)                     []
    (clojure.lang.RT/instance-set? coll) #{}
    (map? coll)                        {}
    (list? coll)                       '()
    (seq? coll)                        '()
    :else nil))

(defn distinct
  "Returns a lazy sequence of the elements of coll with duplicates
  removed. Returns a stateful transducer when no collection is provided."
  ([]
   (fn [rf]
     (let [seen (volatile! #{})]
       (fn
         ([] (rf))
         ([result] (rf result))
         ([result input]
          (if (contains? @seen input)
            result
            (do (vswap! seen conj input)
                (rf result input))))))))
  ([coll]
   (let [step (fn step [xs seen]
                (lazy-seq
                  (loop [s (seq xs)]
                    (when s
                      (let [f (first s)]
                        (if (contains? seen f)
                          (recur (next s))
                          (cons f (step (rest s) (conj seen f)))))))))]
     (step coll #{}))))

(defn replace
  "Given a map of replacement pairs and a vector/collection, returns a
  vector/seq with any elements = a key in smap replaced with the
  corresponding val in smap."
  [smap coll]
  (if (vector? coll)
    (reduce (fn [v i]
              (if (contains? smap (nth v i))
                (assoc v i (get smap (nth v i)))
                v))
            coll (range (count coll)))
    (map (fn [x] (if (contains? smap x) (get smap x) x)) coll)))

(defn mapv
  "Returns a vector consisting of the result of applying f to the set of
  first items of each coll, followed by applying f to the set of second
  items in each coll, until any one of the colls is exhausted."
  ([f coll] (vec (map f coll)))
  ([f c1 c2] (vec (map f c1 c2)))
  ([f c1 c2 c3] (vec (map f c1 c2 c3)))
  ([f c1 c2 c3 & colls] (vec (apply map f c1 c2 c3 colls))))

(defn filterv
  "Returns a vector of the items in coll for which (pred item) returns
  logical true."
  [pred coll] (vec (filter pred coll)))

(defn run!
  "Runs the supplied procedure (via reduce), for purposes of side effects,
  on successive items in the collection. Returns nil."
  [proc coll]
  (reduce #(proc %2) nil coll)
  nil)

(defn map-indexed
  "Returns a lazy sequence consisting of the result of applying f to 0
  and the first item of coll, followed by applying f to 1 and the second
  item in coll, etc. Returns a stateful transducer when no collection
  is provided."
  ([f]
   (fn [rf]
     (let [i (volatile! -1)]
       (fn
         ([] (rf))
         ([result] (rf result))
         ([result input]
          (rf result (f (vswap! i inc) input)))))))
  ([f coll]
   (let [mapi (fn mapi [idx coll]
                (lazy-seq
                  (when-let [s (seq coll)]
                    (cons (f idx (first s)) (mapi (inc idx) (rest s))))))]
     (mapi 0 coll))))

(defn keep-indexed
  "Returns a lazy sequence of the non-nil results of (f index item).
  Note, this means false return values will be included. Returns a
  stateful transducer when no collection is provided."
  ([f]
   (fn [rf]
     (let [iv (volatile! -1)]
       (fn
         ([] (rf))
         ([result] (rf result))
         ([result input]
          (let [i (vswap! iv inc)
                v (f i input)]
            (if (nil? v)
              result
              (rf result v))))))))
  ([f coll]
   (let [keepi (fn keepi [idx coll]
                 (lazy-seq
                   (when-let [s (seq coll)]
                     (let [x (f idx (first s))]
                       (if (nil? x)
                         (keepi (inc idx) (rest s))
                         (cons x (keepi (inc idx) (rest s))))))))]
     (keepi 0 coll))))

(defn subs
  "Returns the substring of s beginning at start inclusive, and ending at
  end (defaults to length of string), exclusive."
  ([s start]     (clojure.lang.RT/subs-impl s start))
  ([s start end] (clojure.lang.RT/subs-impl s start end)))

(defn max-key
  "Returns the x for which (k x), a number, is greatest."
  ([k x] x)
  ([k x y] (if (> (k x) (k y)) x y))
  ([k x y & more]
   (reduce (fn [a b] (max-key k a b)) (max-key k x y) more)))

(defn min-key
  "Returns the x for which (k x), a number, is least."
  ([k x] x)
  ([k x y] (if (< (k x) (k y)) x y))
  ([k x y & more]
   (reduce (fn [a b] (min-key k a b)) (min-key k x y) more)))

(defn bounded-count
  "If coll is counted? returns its count, else will count at most the
  first n elements of coll using its seq."
  [n coll]
  (if (counted? coll)
    (count coll)
    (loop [i 0 s (seq coll)]
      (if (and s (< i n))
        (recur (inc i) (next s))
        i))))

;; --- Nested map ops (vanilla 6209-6280) -----------------------------------

(defn get-in
  "Returns the value in a nested associative structure, where ks is a
  sequence of keys. Returns nil if the key is not present, or the
  not-found value if supplied."
  ([m ks]
   (reduce get m ks))
  ([m ks not-found]
   (loop [sentinel (gensym)
          m m
          ks (seq ks)]
     (if ks
       (let [v (get m (first ks) sentinel)]
         (if (identical? sentinel v)
           not-found
           (recur sentinel v (next ks))))
       m))))

(defn assoc-in
  "Associates a value in a nested associative structure, where ks is a
  sequence of keys and v is the new value and returns a new nested
  structure. If any levels do not exist, hash-maps will be created."
  [m [k & ks] v]
  (let [m (or m {})]
    (if ks
      (assoc m k (assoc-in (get m k) ks v))
      (assoc m k v))))

(defn update-in
  "'Updates' a value in a nested associative structure, where ks is a
  sequence of keys and f is a function that will take the old value
  and any supplied args and return the new value."
  [m ks f & args]
  (let [up (fn up [m ks f args]
             (let [m (or m {})
                   [k & ks] ks]
               (if ks
                 (assoc m k (up (get m k) ks f args))
                 (assoc m k (apply f (get m k) args)))))]
    (up m ks f args)))

(defn update
  "'Updates' a value in an associative structure, where k is a key and
  f is a function that will take the old value and any supplied args
  and return the new value."
  ([m k f]         (assoc m k (f (get m k))))
  ([m k f x]       (assoc m k (f (get m k) x)))
  ([m k f x y]     (assoc m k (f (get m k) x y)))
  ([m k f x y z]   (assoc m k (f (get m k) x y z)))
  ([m k f x y z & more]
   (assoc m k (apply f (get m k) x y z more))))

;; update-vals / update-keys live in the Phase-3 block — they need
;; transient/persistent!/assoc! which are defined later in the file.

;; --- Vector variants (vanilla 1.12: 7426-7470) ----------------------------

(defn splitv-at
  "Returns a vector of [(into [] (take n) coll) (drop n coll)]."
  [n coll]
  [(vec (take n coll)) (drop n coll)])

(defn partitionv
  "Returns a lazy sequence of vectors of n items each, at offsets step
  apart. Like partition but returns vectors instead of seqs."
  ([n coll] (partitionv n n coll))
  ([n step coll]
   (lazy-seq
     (when-let [s (seq coll)]
       (let [p (vec (take n s))]
         (when (= n (count p))
           (cons p (partitionv n step (nthrest s step)))))))))

(defn partitionv-all
  "Returns a lazy sequence of vector partitions, but may include
  partitions with fewer than n items at the end."
  ([n coll] (partitionv-all n n coll))
  ([n step coll]
   (lazy-seq
     (when-let [s (seq coll)]
       (let [seg (vec (take n s))]
         (cons seg (partitionv-all n step (drop step s))))))))

;; --- Transducer extras (vanilla 7827-7890) --------------------------------

(defn halt-when
  "Returns a transducer that ends transduction when pred returns true
  for an input. When retf is supplied it must be a fn of 2 arguments
  taking the (completed) result so far and the input that triggered
  the predicate."
  ([pred] (halt-when pred nil))
  ([pred retf]
   (fn [rf]
     (fn
       ([] (rf))
       ([result]
        (if (and (map? result) (contains? result ::halt))
          (get result ::halt)
          (rf result)))
       ([result input]
        (if (pred input)
          (reduced {::halt (if retf (retf (rf result) input) input)})
          (rf result input)))))))

(defn eduction
  "Returns a reducible/iterable application of the transducers to the
  items in coll. Transducers are applied in order as if combined with
  comp. Simplified: realizes as a lazy seq via `sequence` (loses the
  multiple-iteration property of vanilla's Eduction deftype)."
  [& xforms]
  (sequence (apply comp (butlast xforms)) (last xforms)))

;; --- Type / cast / bytes? / uri? -----------------------------------------

(defn cast
  "Throws a TypeError if x is not an instance of c, else returns x."
  [c x] (clojure.lang.RT/cast-impl c x))

(defn bytes?
  "Return true if x is a Python bytes or bytearray."
  [x] (clojure.lang.RT/bytes-q x))

(defn uri?
  "Return true if x is a urllib.parse ParseResult / SplitResult."
  [x] (clojure.lang.RT/uri-q x))

;; --- Tagged literals (vanilla 7961-7985) ----------------------------------

(defn tagged-literal?
  "Return true if the value is the data representation of a tagged literal."
  [v] (clojure.lang.RT/tagged-literal-q v))

(defn tagged-literal
  "Construct a data representation of a tagged literal from a tag
  symbol and a form."
  [tag form] (clojure._core/TaggedLiteral tag form))

(defn reader-conditional?
  "Return true if the value is the data representation of a reader
  conditional."
  [v] (clojure.lang.RT/reader-conditional-q v))

(defn reader-conditional
  "Construct a data representation of a reader conditional. If true,
  splicing? indicates read-cond-splicing."
  [form splicing?] (clojure._core/ReaderConditional form splicing?))

;; --- ns-imports (vanilla 4230) — Python has no class-import table -------

(defn ns-imports
  "Returns a map of the import mappings for the namespace. Python has
  no Java-style import table — always returns the empty map."
  [_ns] {})

;; --- with-precision (vanilla 5143) ---------------------------------------

(def ^:dynamic *math-context* nil)

(defmacro with-precision
  "Sets the precision (and optionally rounding mode) used by Decimal
  operations within body. Backed by Python's decimal.localcontext()
  via *math-context*."
  [precision & exprs]
  (let [[body rm] (if (= (first exprs) :rounding)
                    [(next (next exprs)) (second exprs)]
                    [exprs nil])]
    `(binding [*math-context* {:precision ~precision :rounding ~(when rm `(quote ~rm))}]
       ~@body)))

;; --- seque (vanilla 5454) — simplified to identity --------------------------

(defn seque
  "Vanilla returns a queued seq pre-fetching ahead of the consumer.
  Our impl returns s unchanged — queueing is a perf optimization, not
  a semantic change. (A real impl would wrap a Python queue.Queue + a
  worker thread.)"
  ([s] s)
  ([_n s] s))

;; --- Agent executor setters (vanilla 2110-2120) ---------------------------

(defn set-agent-send-executor!
  "Sets the executor pool used by `send`. Stub: our pools are
  global OnceCells and aren't currently swappable. Accepts the
  argument and returns nil for code-portability."
  [_executor] nil)

(defn set-agent-send-off-executor!
  "Sets the executor pool used by `send-off`. Stub — see
  set-agent-send-executor!."
  [_executor] nil)

;; --- Auto-promote arithmetic aliases (Python ints already promote) -------

(def +' +)
(def -' -)
(def *' *)
(def inc' inc)
(def dec' dec)

;; (typed Java arrays are defined right after make-array, below)

;; load / test / read+string / definline / add-classpath / compile live
;; in the Phase-3 block (end of file) — they reference later-defined
;; names like load-file, *in*, read.

;; --- Missing macros -------------------------------------------------------

(defmacro defonce
  "defs name to have the root value of the expr iff the named var has no
  root value, else expr is unevaluated."
  [name expr]
  `(let [v# (def ~name)]
     (when-not (clojure.lang.RT/var-bound? v#)
       (def ~name ~expr))))

(defmacro defn-
  "Same as defn, but with :private true metadata."
  [name & decls]
  (list* `defn (with-meta name (assoc (or (meta name) {}) :private true)) decls))

(defmacro comment
  "Ignores body, yields nil."
  [& _] nil)

(defmacro cond->
  "Takes an expression and a set of test/form pairs. Threads expr (via ->)
  through each form for which the corresponding test expression is truthy."
  [expr & clauses]
  (assert (even? (count clauses)))
  (let [g (gensym)
        steps (map (fn [[test step]] `(if ~test (-> ~g ~step) ~g))
                   (partition 2 clauses))]
    `(let [~g ~expr
           ~@(interleave (repeat g) (butlast steps))]
       ~(if (empty? steps) g (last steps)))))

(defmacro cond->>
  "Takes an expression and a set of test/form pairs. Threads expr (via ->>)
  through each form for which the corresponding test expression is truthy."
  [expr & clauses]
  (assert (even? (count clauses)))
  (let [g (gensym)
        steps (map (fn [[test step]] `(if ~test (->> ~g ~step) ~g))
                   (partition 2 clauses))]
    `(let [~g ~expr
           ~@(interleave (repeat g) (butlast steps))]
       ~(if (empty? steps) g (last steps)))))

(defmacro as->
  "Binds name to expr, evaluates the first form in the lexical context
  of that binding, then binds name to that result, repeating for each
  successive form, returning the result of the last form."
  [expr name & forms]
  `(let [~name ~expr
         ~@(interleave (repeat name) (butlast forms))]
     ~(if (empty? forms) name (last forms))))

(defmacro some->
  "When expr is not nil, threads it into the first form (via ->), and when
  that result is not nil, through the next, etc."
  [expr & forms]
  (let [g (gensym)
        steps (map (fn [step] `(if (nil? ~g) nil (-> ~g ~step))) forms)]
    `(let [~g ~expr
           ~@(interleave (repeat g) (butlast steps))]
       ~(if (empty? steps) g (last steps)))))

(defmacro some->>
  "When expr is not nil, threads it into the first form (via ->>), and
  when that result is not nil, through the next, etc."
  [expr & forms]
  (let [g (gensym)
        steps (map (fn [step] `(if (nil? ~g) nil (->> ~g ~step))) forms)]
    `(let [~g ~expr
           ~@(interleave (repeat g) (butlast steps))]
       ~(if (empty? steps) g (last steps)))))

(defn with-redefs-fn
  "Temporarily redefines Vars during a call to func. Each Var will be
  reset to its ROOT value when func returns (not any currently-active
  thread binding) — matches vanilla's behavior so nested `(binding …)`
  doesn't leak into the post-restoration root."
  [binding-map func]
  (let [vars (keys binding-map)
        original-values (zipmap vars (map clojure.lang.RT/get-root vars))]
    (try
      (doseq [[v new-val] binding-map]
        (clojure.lang.RT/bind-root v new-val))
      (func)
      (finally
        (doseq [[v old-val] original-values]
          (clojure.lang.RT/bind-root v old-val))))))

(defmacro with-redefs
  "binding => var-symbol temp-value-expr
  Temporarily redefines vars while executing the body. Restores them at
  exit, even if body throws."
  [bindings & body]
  `(with-redefs-fn
     ~(zipmap (map (fn [v] `(var ~v)) (take-nth 2 bindings))
              (take-nth 2 (next bindings)))
     (fn [] ~@body)))

;; --- I/O & string formatting (forward-ref-free subset) --------------------

(defn format
  "Formats a string using java.lang.String.format (here: Python's % operator).
  Translates %n to newline."
  [fmt & args]
  (apply clojure.lang.RT/format-impl fmt args))

(defn slurp
  "Opens a reader on f and reads all its contents, returning a string."
  [f] (clojure.lang.RT/slurp-impl f))

(defn spit
  "Opposite of slurp. Writes content to a file."
  [f content] (clojure.lang.RT/spit-impl f content))

(defn iterator-seq
  "Returns a seq on a python iterator. Note that most collections are
  iterable, so this should rarely be needed — prefer (seq coll) directly."
  [it] (clojure.lang.RT/iterator-seq-impl it))

(defn enumeration-seq
  "Like iterator-seq, but for python iterators (Python has no
  Enumeration class — alias for iterator-seq)."
  [e] (clojure.lang.RT/iterator-seq-impl e))

(defn file-seq
  "A tree seq on files starting at path, returning all paths
  (directories first, then files) walked in pre-order."
  [path] (clojure.lang.RT/file-seq-impl path))

(defn xml-seq
  "A tree seq on the XML elements as per xml.etree.ElementTree. The
  argument should be an Element; iterates direct children via list()."
  [root]
  (tree-seq
    (fn [n] (boolean (clojure.lang.RT/xml-children n)))
    (fn [n] (clojure.lang.RT/xml-children n))
    root))

;; --- Random ---------------------------------------------------------------

(defn rand
  "Returns a random floating point number between 0 (inclusive) and n
  (default 1) (exclusive)."
  ([] (clojure.lang.RT/rand-impl))
  ([n] (* n (clojure.lang.RT/rand-impl))))

(defn rand-int
  "Returns a random integer between 0 (inclusive) and n (exclusive)."
  [n] (clojure.lang.RT/rand-int-impl n))

(defn rand-nth
  "Return a random element of the (sequential) collection. Will have
  the same performance characteristics as nth for the given collection."
  [coll] (nth coll (rand-int (count coll))))

;; --- Var plumbing ---------------------------------------------------------

(defn eval
  "Evaluates the form data structure (not text!) and returns the result."
  [form] (clojure.lang.RT/eval-form form))

(defn alter-var-root
  "Atomically alters the root binding of var v by applying f to its
  current value plus any args. Returns the new value."
  [v f & args]
  (apply clojure.lang.RT/alter-var-root v f args))

(defmacro import
  "Python equivalent: a no-op stub. Vanilla `import` brings JVM classes
  into a namespace's import table; in Python, classes are module
  attributes — use `(.somemethod ...)` and qualified-symbol resolution
  instead."
  [& _] nil)

;; --- Misc utilities --------------------------------------------------------

(defn re-matcher
  "Returns an stateful matcher object for use with re-find. Vanilla
  returns java.util.regex.Matcher; we wrap Python's re.Pattern.finditer."
  [re s] (clojure.lang.RT/re-matcher-impl re s))

(defn re-groups
  "Returns the groups from the most recent match (use with re-matcher)."
  [m] (clojure.lang.RT/re-groups-impl m))

(defn shuffle
  "Return a random permutation of coll."
  [coll] (clojure.lang.RT/shuffle-impl coll))

(defn hash
  "Returns the hash code of its argument. Note this is the hash code
  consistent with =, but per-type hashing for Java equals semantics is
  delegated to native Python hash."
  [x] (clojure.lang.RT/hash-eq x))

(defn mix-collection-hash
  "Mix the hash of a collection's elements with its count."
  [hash-basis count] (clojure.lang.RT/mix-collection-hash hash-basis count))

(defn hash-ordered-coll
  "Returns the hash code, consistent with =, for an external ordered
  collection."
  [coll] (clojure.lang.RT/hash-ordered-coll-impl coll))

(defn hash-unordered-coll
  "Returns the hash code, consistent with =, for an external unordered
  collection."
  [coll] (clojure.lang.RT/hash-unordered-coll-impl coll))

(defn bases
  "Returns the immediate superclass and direct interfaces of c (via Python
  __bases__)."
  [c] (clojure.lang.RT/bases-impl c))

(defn supers
  "Returns the immediate and indirect superclasses and interfaces of c
  (via Python's MRO, excluding c itself)."
  [c] (clojure.lang.RT/supers-impl c))

(defn Throwable->map
  "Constructs a data representation for a throwable: {:cause :via :trace
  [:data]}."
  [t] (clojure.lang.RT/throwable-to-map t))

;; --- Tap system -----------------------------------------------------------

(defn add-tap
  "Adds f, a fn of one argument, to the tap set. This function will be
  called by tap> with values it receives."
  [f] (clojure.lang.RT/add-tap-impl f))

(defn remove-tap
  "Remove f from the tap set."
  [f] (clojure.lang.RT/remove-tap-impl f))

(defn tap>
  "Sends x to any taps. Returns true if there was room, false otherwise."
  [x] (clojure.lang.RT/tap-bang-impl x))

;; --- case (vanilla 6793) --------------------------------------------------
;;
;; Vanilla case uses a JVM-specific tableswitch/lookupswitch opcode for
;; constant-time dispatch. We expand to a `condp =` chain (linear in
;; clauses) — same semantics, slower for very wide cases. For the typical
;; few-clause case this is indistinguishable.

(defmacro case
  "Takes an expression and a set of clauses.
  Each clause is of the form: test-constant result-expr
  Or: (test-constant1 ... test-constantN) result-expr
  An optional final expr is the default."
  [e & clauses]
  (let [g    (gensym)
        ;; Split into [test-result pairs] + optional default.
        has-default? (odd? (count clauses))
        pairs (if has-default? (butlast clauses) clauses)
        default (when has-default? (last clauses))
        ;; Each pair becomes a chain of (if (= g <const>) result …)
        chain (reduce
                (fn [acc [t r]]
                  (if (and (seq? t) (not= 'quote (first t)))
                    ;; List of test constants — match any.
                    (let [tests (vec t)]
                      `(if (or ~@(map (fn [c] `(= ~g (quote ~c))) tests))
                         ~r
                         ~acc))
                    `(if (= ~g (quote ~t)) ~r ~acc)))
                (if has-default?
                  default
                  `(clojure.lang.RT/throw-iae (str "No matching clause: " ~g)))
                (reverse (partition 2 pairs)))]
    `(let [~g ~e] ~chain)))

;; ============================================================================
;; Phase-2 alignment: forms filling gaps through vanilla line ~4680.
;; Grouped by cohort; each block cites its vanilla source line range.
;; ============================================================================

;; --- Transients (vanilla 3364-3430) ---
;;
;; Editable snapshots of persistent collections. The Rust runtime already
;; carries the `ITransient*` protocols and per-type impls; these are thin
;; Clojure-level wrappers delegating to `RT/*-bang` shims.

(defn transient
  "Returns a new, transient version of the collection, in constant time."
  [coll]
  (clojure.lang.RT/transient coll))

(defn persistent!
  "Returns a new, persistent version of the transient collection, in
  constant time. The transient collection cannot be used after this call,
  any such use will throw an exception."
  [coll]
  (clojure.lang.RT/persistent-bang coll))

(defn conj!
  "Adds x to the transient collection, and return coll. The 'addition'
  may happen at different 'places' depending on the concrete type."
  ([] (transient []))
  ([coll] coll)
  ([coll x] (clojure.lang.RT/conj-bang coll x)))

(defn assoc!
  "When applied to a transient map, adds mapping of key(s) to val(s).
  When applied to a transient vector, sets the val at index. Note — index
  must be <= (count vector)."
  ([coll key val] (clojure.lang.RT/assoc-bang coll key val))
  ([coll key val & kvs]
    (let [ret (clojure.lang.RT/assoc-bang coll key val)]
      (if kvs
        (if (next kvs)
          (recur ret (first kvs) (second kvs) (nnext kvs))
          (clojure.lang.RT/throw-iae
            "assoc! expects even number of arguments after map, found odd number"))
        ret))))

(defn dissoc!
  "Returns a transient map that doesn't contain a mapping for key(s)."
  ([coll key] (clojure.lang.RT/dissoc-bang coll key))
  ([coll key & ks]
    (let [ret (clojure.lang.RT/dissoc-bang coll key)]
      (if ks
        (recur ret (first ks) (next ks))
        ret))))

(defn pop!
  "Removes the last item from a transient vector. If the collection is
  empty, throws an exception. Returns coll."
  [coll]
  (clojure.lang.RT/pop-bang coll))

(defn disj!
  "disj[oin]. Returns a transient set of the same (hashed/sorted) type,
  that does not contain key(s)."
  ([coll] coll)
  ([coll key] (clojure.lang.RT/disj-bang coll key))
  ([coll key & ks]
    (let [ret (clojure.lang.RT/disj-bang coll key)]
      (if ks
        (recur ret (first ks) (next ks))
        ret))))

;; --- Collection constructors + predicates (vanilla 4129-4409) ---

(defn set?
  "Returns true if x implements IPersistentSet"
  [x]
  (clojure.lang.RT/instance-set? x))

(defn set
  "Returns a set of the distinct elements of coll."
  [coll]
  (if (set? coll)
    (with-meta coll nil)
    (persistent! (reduce conj! (transient #{}) coll))))

(defn array-map
  "Constructs an array-map. If any keys are equal, they are handled as
  if by repeated uses of assoc."
  [& keyvals]
  (clojure.lang.RT/apply clojure.lang.RT/array-map keyvals))

(defn subvec
  "Returns a persistent vector of the items in vector from start
  (inclusive) to end (exclusive). If end is not supplied, defaults to
  (count vector)."
  ([v start]
    (subvec v start (count v)))
  ([v start end]
    ;; Placeholder: a SubVector pyclass would be O(1). For now, eager
    ;; copy via take/drop — still persistent, still correct.
    (vec (take (- end start) (drop start v)))))

;; --- rseq (vanilla 1600) / replicate (vanilla 3033) ---

(defn rseq
  "Returns, in constant time, a seq of the items in rev (which can be a
  vector or sorted-map), in reverse order. If rev is empty returns nil."
  [rev]
  (clojure.lang.RT/rseq rev))

(defn replicate
  "DEPRECATED: Use 'repeat' instead. Returns a lazy seq of n xs."
  [n x] (take n (repeat x)))

;; --- Numeric (vanilla 3503, 3589, 3596) ---

(defn num
  "Coerce to Number."
  [x]
  (clojure.lang.RT/num x))

;; `number?` already defined at line ~863 — delegates to RT/number?, which
;; covers int/float/Fraction/Decimal (updated by the numeric-tower chunk).

(defn mod
  "Modulus of num and div. Truncates toward negative infinity."
  [num div]
  (let [m (rem num div)]
    (if (or (zero? m) (= (pos? num) (pos? div)))
      m
      (+ m div))))

;; --- Small macros (vanilla 2733, 2797, 3861, 3882, 3901, 3914, 4667) ---

(defmacro dotimes
  "bindings => name n
  Repeatedly executes body (presumably for side-effects) with name
  bound to integers from 0 through n-1."
  [bindings & body]
  (assert-args
    (vector? bindings) "dotimes requires a vector for its binding"
    (= 2 (count bindings)) "dotimes requires exactly 2 forms in binding vector")
  (let [i (bindings 0) n (bindings 1)]
    `(let [n# ~n]
       (loop [~i 0]
         (when (< ~i n#)
           ~@body
           (recur (inc ~i)))))))

(defmacro declare
  "defs the supplied var names with no bindings, useful for making
  forward declarations."
  [& names]
  `(do
     ~@(map (fn [nm] (list 'def nm)) names)))

(defmacro lazy-cat
  "Expands to code which yields a lazy sequence of the concatenation of
  the supplied colls. Each coll expr is not evaluated until it is needed."
  [& colls]
  `(concat ~@(map (fn [c] `(lazy-seq ~c)) colls)))

(defmacro with-open
  "bindings => [name init ...]
  Evaluates body in a try expression with names bound to the values of
  the inits, and a finally clause that calls (.close name) on each name
  in reverse order."
  [bindings & body]
  (assert-args
    (vector? bindings) "with-open requires a vector for its binding"
    (even? (count bindings)) "with-open requires an even number of forms in binding vector")
  (cond
    (= (count bindings) 0) `(do ~@body)
    (symbol? (bindings 0))
      `(let ~(subvec bindings 0 2)
         (try
           (with-open ~(subvec bindings 2) ~@body)
           (finally
             (. ~(bindings 0) close))))
    :else (clojure.lang.RT/throw-iae
            "with-open only allows Symbols in bindings")))

(defmacro doto
  "Evaluates x then calls all of the methods and functions with the
  value of x supplied at the front of the given arguments.  The forms
  are evaluated in order.  Returns x."
  [x & forms]
  (let [gx (gensym "dotox_")]
    `(let [~gx ~x]
       ~@(map (fn [f]
                (if (seq? f)
                  `(~(first f) ~gx ~@(next f))
                  `(~f ~gx)))
              forms)
       ~gx)))

(defmacro memfn
  "Expands into code that creates a fn that expects to be passed an
  object and any args and calls the named instance method on the object
  passing the args. Use when you want to treat a Java/Python method as
  a first-class fn."
  [name & args]
  (let [t (gensym "target_")
        p (map (fn [_] (gensym "arg_")) args)]
    `(fn [~t ~@p]
       (. ~t ~(symbol (str name)) ~@p))))

(defmacro time
  "Evaluates expr and prints the time it took.  Returns the value of expr.
  Printing deferred until the print-method chunk lands — for now the macro
  measures elapsed time but returns only the value; the string is discarded."
  [expr]
  `(let [start# (clojure.lang.RT/time-ns)
         ret# ~expr
         end# (clojure.lang.RT/time-ns)]
     ret#))

;; --- macroexpand (vanilla 4048-4066) ---

(defn macroexpand-1
  "If form represents a macro form, returns its expansion, else returns form."
  [form]
  (clojure.lang.RT/macroexpand-1 form))

(defn macroexpand
  "Repeatedly calls macroexpand-1 on form until it no longer represents
  a macro form, then returns it.  Note neither macroexpand-1 nor
  macroexpand expand macros in subforms."
  [form]
  (let [ex (macroexpand-1 form)]
    (if (identical? ex form)
      form
      (macroexpand ex))))

;; --- Namespace basics (vanilla 4156-4201) ---

(defn find-ns
  "Returns the namespace named by the symbol or nil if it doesn't exist."
  [sym]
  (clojure.lang.RT/find-ns sym))

(defn create-ns
  "Create a new namespace named by the symbol if one doesn't already
  exist, returns it or the already-existing namespace of the same name."
  [sym]
  (clojure.lang.RT/create-ns sym))

(defn the-ns
  "If passed a namespace, returns it. Else, when passed a symbol,
  returns the namespace named by it, throwing an exception if not found."
  [x]
  (clojure.lang.RT/the-ns x))

(defn ns-name
  "Returns the name of the namespace, a symbol."
  [ns]
  (clojure.lang.RT/ns-name ns))

;; --- Vars (vanilla 4357-4370) ---

(defn find-var
  "Returns the global var named by the namespace-qualified symbol, or nil
  if no var with that name."
  [sym]
  (clojure.lang.RT/find-var sym))

(defn var-get
  "Gets the value in the var object"
  [x]
  (clojure.lang.RT/var-get x))

(defn var-set
  "Sets the value in the var object to val. The var must be thread-locally
  bound."
  [v val] (clojure.lang.RT/var-set-bang v val))

(defmacro with-local-vars
  "varbinding => symbol init-expr

  Executes the exprs in a context in which the symbols are bound to
  vars with per-thread bindings to the init-exprs. The symbols refer to
  the var objects themselves, and must be accessed with var-get and
  var-set!"
  [name-vals-vec & body]
  (let [names (take-nth 2 name-vals-vec)
        vals  (take-nth 2 (next name-vals-vec))]
    `(let [~@(interleave names (repeat (list 'clojure.lang.RT/var-create)))]
       (clojure.lang.RT/push-thread-bindings
         (hash-map ~@(interleave names vals)))
       (try
         (do ~@body)
         (finally (clojure.lang.RT/pop-thread-bindings))))))

;; --- resolve (vanilla 4389-4402) ---

(defn ns-resolve
  "Returns the var or Class to which a symbol will be resolved in the
  namespace, else nil. Class resolution (for dotted paths) is deferred."
  ([ns sym]
    (let [n (the-ns ns)
          v (clojure.lang.RT/getattr n (name sym) nil)]
      (if (clojure.lang.RT/instance-var? v) v nil)))
  ([ns env sym]
    (ns-resolve ns sym)))

(defn resolve
  "same as (ns-resolve *ns* symbol) or (ns-resolve *ns* &env symbol)"
  ([sym] (ns-resolve *ns* sym))
  ([env sym] (ns-resolve *ns* env sym)))

;; --- Watches / validators / meta ops on IReference types
;; (vanilla 2165-2437) ---

(defn add-watch
  "Adds a watch function to an agent/atom/var/ref reference. The watch
  fn must be a fn of 4 args: a key, the reference, its old-state, its
  new-state. Whenever the reference's state changes, any registered
  watches will have their functions called. The watch fn's return value
  is ignored."
  [reference key fn]
  (clojure.lang.RT/add-watch reference key fn))

(defn remove-watch
  "Removes a watch (set by add-watch) from a reference"
  [reference key]
  (clojure.lang.RT/remove-watch reference key))

(defn set-validator!
  "Sets the validator-fn for a var/ref/agent/atom. validator-fn must be
  nil or a side-effect-free fn of one argument, which will be passed
  the intended new state on any state change. If the new state is
  unacceptable, the validator-fn should return false or throw an
  exception. If the current state is unacceptable when setting the
  validator, an exception will be thrown and the validator will not
  be changed."
  [iref validator-fn]
  (clojure.lang.RT/set-validator-bang iref validator-fn))

(defn get-validator
  "Gets the validator-fn for a var/ref/agent/atom."
  [iref]
  (clojure.lang.RT/get-validator iref))

(defn alter-meta!
  "Atomically sets the metadata for a namespace/var/ref/agent/atom to
  be: (apply f its-current-meta args). f must be free of side-effects."
  [iref f & args]
  (clojure.lang.RT/apply clojure.lang.RT/alter-meta-bang iref f args))

(defn reset-meta!
  "Atomically resets the metadata for a namespace/var/ref/agent/atom."
  [iref metadata-map]
  (clojure.lang.RT/reset-meta-bang iref metadata-map))

;; --- Monitors + locking (vanilla ~2900) ---

(defn monitor-enter [x]
  (clojure.lang.RT/monitor-enter x))

(defn monitor-exit [x]
  (clojure.lang.RT/monitor-exit x))

(defmacro locking
  "Executes exprs in an implicit do, while holding the monitor of x.
  Will release the monitor of x in all circumstances."
  [x & body]
  `(let [lockee# ~x]
     (monitor-enter lockee#)
     (try
       ~@body
       (finally
         (monitor-exit lockee#)))))

;; --- read-string (vanilla 3835) ---

(defn read-string
  "Reads one object from the string s."
  [s]
  (clojure.lang.RT/read-string s))

;; --- Dynamic bindings (vanilla 1934-2043) ---

(defn push-thread-bindings
  "WARNING: This is a low-level function. Prefer high-level macros like
  binding where ever possible.

  Takes a map of Var/value pairs. Binds each Var to the associated value
  for the current thread. Each call *MUST* be accompanied by a matching
  call to pop-thread-bindings wrapped in a try-finally!"
  [bindings]
  (clojure.lang.RT/push-thread-bindings bindings))

(defn pop-thread-bindings
  "Pop one set of bindings pushed with push-binding before. It is an
  error to pop bindings without pushing before."
  []
  (clojure.lang.RT/pop-thread-bindings))

(defn get-thread-bindings
  "Get a map with the Var/value pairs which is currently in effect for
  the current thread."
  []
  (clojure.lang.RT/get-thread-bindings))

(defn with-bindings*
  "Takes a map of Var/value pairs. Installs for the given Vars the
  associated values as thread-local bindings. Then calls f with the
  supplied arguments. Pops the installed bindings after f returned.
  Returns whatever f returns."
  [binding-map f & args]
  (push-thread-bindings binding-map)
  (try
    (apply f args)
    (finally
      (pop-thread-bindings))))

(defmacro with-bindings
  "Takes a map of Var/value pairs. Installs for the given Vars the
  associated values as thread-local bindings. Then executes body.
  Pops the installed bindings after body was evaluated. Returns the
  value of body."
  [binding-map & body]
  `(with-bindings* ~binding-map (fn [] ~@body)))

(defmacro binding
  "binding => var-symbol init-expr
  Creates new bindings for the (already-existing) vars, with the
  supplied initial values, executes the exprs in an implicit do,
  then re-establishes the bindings that existed before.  The new
  bindings are made in parallel (unlike let); all init-exprs are
  evaluated before the vars are bound to their new values."
  [bindings & body]
  (assert-args
    (vector? bindings) "binding requires a vector for its binding"
    (even? (count bindings)) "binding requires an even number of forms in binding vector")
  (let [var-ize (fn [var-vals]
                  (loop [ret [] vvs (seq var-vals)]
                    (if vvs
                      (recur (conj (conj ret (list 'var (first vvs))) (second vvs))
                             (next (next vvs)))
                      (seq ret))))]
    `(let []
       (push-thread-bindings (hash-map ~@(var-ize bindings)))
       (try
         ~@body
         (finally
           (pop-thread-bindings))))))

(defn bound-fn*
  "Returns a function, which will install the same bindings in effect
  as in the thread at the time bound-fn* was called and then call f
  with any given arguments. This may be used to define a helper
  function which runs on a different thread, but needs the same
  bindings in place."
  [f]
  (let [bindings (get-thread-bindings)]
    (fn [& args]
      (apply with-bindings* bindings f args))))

(defmacro bound-fn
  "Returns a function defined by the given fntail, which will install
  the same bindings in effect as in the thread at the time bound-fn
  was called. This may be used to define a helper function which runs
  on a different thread, but needs the same bindings in place."
  [& fntail]
  `(bound-fn* (fn ~@fntail)))

(defn ^:private binding-conveyor-fn
  "Returns a function that, when invoked, first installs the full
  thread-binding frame captured at the time of this call and then invokes
  f with the given args. Used by `send`/`send-off` to convey bindings to
  an agent action running on a worker thread."
  [f]
  (let [frame (clojure.lang.RT/clone-thread-binding-frame)]
    (fn
      ([]
         (clojure.lang.RT/reset-thread-binding-frame frame)
         (f))
      ([x]
         (clojure.lang.RT/reset-thread-binding-frame frame)
         (f x))
      ([x y]
         (clojure.lang.RT/reset-thread-binding-frame frame)
         (f x y))
      ([x y z]
         (clojure.lang.RT/reset-thread-binding-frame frame)
         (f x y z))
      ([x y z & args]
         (clojure.lang.RT/reset-thread-binding-frame frame)
         (apply f x y z args)))))

;; --- Numeric tower (vanilla 3503-3691) ---
;;
;; Python's native numeric types cover the full vanilla tower with
;; aliasing: int (arbitrary precision) for Long/Integer/Short/Byte/BigInt,
;; float (IEEE double) for Double/Float, fractions.Fraction for Ratio,
;; decimal.Decimal for BigDecimal. The width casts collapse to int()/float()
;; since Python has no fixed-width integer types; the unchecked-* variants
;; alias their checked counterparts because Python int can't overflow.

(defn ratio?
  "Returns true if n is a Ratio (fractions.Fraction)."
  [n] (clojure.lang.RT/instance-ratio? n))

(defn decimal?
  "Returns true if n is a BigDecimal (decimal.Decimal)."
  [n] (clojure.lang.RT/instance-decimal? n))

(defn float?
  "Returns true if n is a floating-point number (Python float)."
  [n] (clojure.lang.RT/instance-float? n))

(defn rational?
  "Returns true if n is a rational number (int, Ratio, or BigDecimal)."
  [n] (clojure.lang.RT/instance-rational? n))

;; `integer?` / `int?` / `int` already defined earlier in the file.

(defn numerator
  "Returns the numerator part of a Ratio. For int, returns itself."
  [q] (clojure.lang.RT/numerator q))

(defn denominator
  "Returns the denominator part of a Ratio. For int, returns 1."
  [q] (clojure.lang.RT/denominator q))

(defn bigint
  "Coerce to BigInt (Python int — arbitrary precision)."
  [x] (clojure.lang.RT/bigint x))

(defn biginteger
  "Coerce to BigInteger (Python int)."
  [x] (clojure.lang.RT/biginteger x))

(defn bigdec
  "Coerce to BigDecimal (decimal.Decimal)."
  [x] (clojure.lang.RT/bigdec x))

(defn rationalize
  "Returns the rational value of num as a Ratio (or int if exact)."
  [num] (clojure.lang.RT/rationalize num))

;; --- Width casts (vanilla 3510-3582) ---
;;
;; Python has one arbitrary-precision int and one IEEE-double float; no
;; narrower integer types and no single-precision float. All integer-width
;; casts collapse to int(); all float casts collapse to float(). The
;; unchecked-* variants alias their checked counterparts since Python
;; int can't overflow.

(defn long   "Coerce to long (Python int)."   [x] (clojure.lang.RT/bigint x))
;; `int` already defined at line 580; don't redefine.
(defn short  "Coerce to short (Python int)."  [x] (clojure.lang.RT/bigint x))
(defn byte   "Coerce to byte (Python int)."   [x] (clojure.lang.RT/bigint x))
(defn char   "Coerce to char (Python int)."   [x] (clojure.lang.RT/bigint x))
(defn double "Coerce to double (Python float)."
  [x] (clojure.lang.RT/float-coerce x))
(defn float  "Coerce to float (Python float)."
  [x] (clojure.lang.RT/float-coerce x))

(defn unchecked-byte   "Unchecked byte cast (= byte for Python)."   [x] (byte x))
(defn unchecked-short  "Unchecked short cast (= short for Python)." [x] (short x))
(defn unchecked-char   "Unchecked char cast (= char for Python)."   [x] (char x))
(defn unchecked-int    "Unchecked int cast (= int for Python)."     [x] (int x))
(defn unchecked-long   "Unchecked long cast (= long for Python)."   [x] (long x))
(defn unchecked-float  "Unchecked float cast (= float for Python)." [x] (float x))
(defn unchecked-double "Unchecked double cast (= double for Python)." [x] (double x))

;; --- Type / class (vanilla 3497-3501) ---

(defn class?
  "Returns true if x is a Python class/type (JVM Class analogue)."
  [x] (clojure.lang.RT/class? x))

(defn class
  "Returns the class of x. For Python this is type(x)."
  [x] (clojure.lang.RT/class x))

(defn instance?
  "Returns true if x is an instance of class c (Python isinstance)."
  [c x] (clojure.lang.RT/instance? c x))

(defn type
  "Returns the :type metadata of x, or its class, or nil if x is nil."
  [x]
  (if (nil? x)
    nil
    (or (when (clojure.lang.RT/instance-imeta? x)
          (get (meta x) :type))
        (class x))))

;; --- Hierarchy (vanilla 1665-1720) ---
;;
;; A hierarchy is an immutable map with keys `:parents`, `:ancestors`,
;; `:descendants`, each a map from tag → set. The global hierarchy is a
;; dynamic Var whose root is an Atom holding the current map; `derive` /
;; `underive` swap new values in atomically.

(defn make-hierarchy
  "Creates a hierarchy object for use with derive, isa?, etc."
  []
  {:parents {} :ancestors {} :descendants {}})

(def ^{:private false
       :doc "The global hierarchy used by multimethods. Updated
            atomically via `derive` / `underive` (alter-var-root)."}
  global-hierarchy
  (make-hierarchy))

(defn ^:private tf
  "Helper for derive: walks source and its sources, assoc'ing target +
  its targets into m keyed by each encountered node. Mirrors vanilla's
  inner-letfn of the same name."
  [m source sources target targets]
  (reduce (fn [ret k]
            (assoc ret k
                   (reduce conj (get targets k #{})
                           (cons target (targets target)))))
          m (cons source (sources source))))

(defn derive
  "Establishes a parent/child relationship between parent and tag.
  Parent must be a keyword or symbol; tag must be a keyword or symbol,
  or (in the 2-arg case) a Python class."
  ([tag parent]
    (assert (not= tag parent) "(not= tag parent)")
    (alter-var-root #'global-hierarchy derive tag parent)
    nil)
  ([h tag parent]
    (assert (not= tag parent) "(not= tag parent)")
    (let [tp (:parents h)
          td (:descendants h)
          ta (:ancestors h)]
      (if (contains? (get tp tag #{}) parent)
        h
        (do
          (when (contains? (get ta tag #{}) parent)
            (clojure.lang.RT/throw-iae
              (clojure.lang.RT/str-concat (str tag)
                (clojure.lang.RT/str-concat " already has "
                  (clojure.lang.RT/str-concat (str parent) " as ancestor")))))
          (when (contains? (get ta parent #{}) tag)
            (clojure.lang.RT/throw-iae
              (clojure.lang.RT/str-concat "Cyclic derivation: "
                (clojure.lang.RT/str-concat (str parent)
                  (clojure.lang.RT/str-concat " has "
                    (clojure.lang.RT/str-concat (str tag) " as ancestor"))))))
          {:parents (assoc tp tag (conj (get tp tag #{}) parent))
           :ancestors (tf ta tag td parent ta)
           :descendants (tf td parent ta tag td)})))))

(defn underive
  "Removes a parent/child relationship between parent and tag."
  ([tag parent]
    (alter-var-root #'global-hierarchy underive tag parent)
    nil)
  ([h tag parent]
    (let [parent-map (get (:parents h) tag)]
      (if (contains? (or parent-map #{}) parent)
        ;; Rebuild from scratch by replaying every remaining parent edge.
        (let [new-parents (reduce (fn [m kv]
                                    (let [k (key kv) ps (val kv)]
                                      (if (seq ps) (assoc m k ps) m)))
                                  {}
                                  (assoc (:parents h) tag (disj parent-map parent)))
              new-h (reduce (fn [h kv]
                              (let [k (key kv) ps (val kv)]
                                (reduce (fn [h2 p] (derive h2 k p)) h ps)))
                            (make-hierarchy) new-parents)]
          new-h)
        h))))

(defn parents
  "Returns the immediate parents of tag, either via a Python class or in
  the hierarchy."
  ([tag] (parents global-hierarchy tag))
  ([h tag]
    (let [ps (get (:parents h) tag)]
      (if (class? tag)
        (reduce conj (or ps #{}) (clojure.lang.RT/class-bases tag))
        ps))))

(defn ancestors
  "Returns the immediate and indirect ancestors of tag, either via a
  Python class or the hierarchy."
  ([tag] (ancestors global-hierarchy tag))
  ([h tag]
    (let [as (get (:ancestors h) tag)]
      (if (class? tag)
        (reduce conj (or as #{}) (clojure.lang.RT/class-ancestors tag))
        as))))

(defn descendants
  "Returns the immediate and indirect descendants of tag. Note: does NOT
  include Python subclasses (since Python has no registry of subclasses)."
  ([tag] (descendants global-hierarchy tag))
  ([h tag]
    (when (class? tag)
      (clojure.lang.RT/throw-iae
        "Can't get descendants of a Python class"))
    (get (:descendants h) tag)))

(defn isa?
  "Returns true if (= child parent), or child is directly or indirectly
  derived from parent via derive, or a Python subclass, or both are
  vectors of the same length with elementwise isa?."
  ([child parent] (isa? global-hierarchy child parent))
  ([h child parent]
    (or (= child parent)
        (contains? (get (:ancestors h) child #{}) parent)
        (clojure.lang.RT/isa-class? child parent)
        (and (vector? child) (vector? parent)
             (= (count child) (count parent))
             (loop [i 0]
               (cond
                 (= i (count child)) true
                 (not (isa? h (nth child i) (nth parent i))) false
                 :else (recur (inc i))))))))

;; --- Multimethods (vanilla 1746-1845) ---

(defmacro defmulti
  "Creates a new multimethod with the associated dispatch function.
  Options include :default (dispatch value), :hierarchy (hierarchy var).
  Redefinition warns per vanilla; here it just re-uses the existing Var."
  [mm-name & options]
  (let [docstring (if (string? (first options)) (first options) nil)
        options (if docstring (next options) options)
        attr-map (if (map? (first options)) (first options) nil)
        options (if attr-map (next options) options)
        dispatch-fn (first options)
        options (next options)
        ;; parse :default and :hierarchy from trailing options
        option-map (apply hash-map options)
        default-val (get option-map :default :default)
        hierarchy-var (get option-map :hierarchy '(var global-hierarchy))
        meta-map (merge (or attr-map {})
                        (if docstring {:doc docstring} {}))]
    `(let [v# (def ~mm-name)]
       (when-not (and (.-is_bound v#)
                      (clojure.lang.RT/instance-multifn? (clojure.lang.RT/var-get v#)))
         (def ~(with-meta mm-name meta-map)
              (clojure.lang.RT/multifn-create
                ~(str mm-name)
                ~dispatch-fn
                ~default-val
                ~hierarchy-var)))
       v#)))

(defmacro defmethod
  "Creates and installs a new method of multimethod associated with
  dispatch-value."
  [multifn dispatch-val & fn-tail]
  `(.addMethod ~(with-meta multifn {:tag 'clojure.lang.MultiFn})
               ~dispatch-val (fn ~@fn-tail)))

(defn remove-all-methods
  "Removes all of the methods of multimethod."
  [multifn]
  (.removeAllMethods multifn))

(defn remove-method
  "Removes the method of multimethod associated with dispatch-value."
  [multifn dispatch-val]
  (.removeMethod multifn dispatch-val))

(defn prefer-method
  "Causes the multimethod to prefer matches of dispatch-val-x over
  dispatch-val-y when there is a conflict."
  [multifn dispatch-val-x dispatch-val-y]
  (.preferMethod multifn dispatch-val-x dispatch-val-y))

(defn methods
  "Given a multimethod, returns a map of dispatch values -> dispatch fns."
  [multifn]
  (.methodTable multifn))

(defn get-method
  "Given a multimethod and a dispatch value, returns the dispatch fn
  that would apply to that value, or nil if none apply and no default."
  [multifn dispatch-val]
  (.getMethod multifn dispatch-val))

(defn prefers
  "Given a multimethod, returns a map of preferred value -> set of
  other values."
  [multifn]
  (.preferTable multifn))

;; --- Protocols (vanilla 5050-5250) ---
;;
;; `defprotocol` creates a Protocol object and a callable dispatcher Var for
;; each method. Protocols are first-class: they're just `Protocol` pyclass
;; instances, and participate in the same dispatch machinery as Rust-defined
;; `#[protocol]` traits — exact PyType lookup → MRO walk → optional
;; `__clj_meta__` → optional fallback.
;;
;; `extend-type` / `extend-protocol` register impls on existing types (Rust
;; pyclasses, Clojure records, plain Python classes) by calling
;; `Protocol.extend_type(target, {method-name: impl-fn})`.

(defn ^:private extend-type-groups
  "Split the specs passed to extend-type/extend-protocol into
  `[[protocol-form [(m-name [params] body…) …]] …]`. Each protocol-form
  is anything that isn't a seq; each seq is a method impl under the
  most-recent protocol-form."
  [specs]
  (loop [remaining (seq specs)
         current   nil
         impls     []
         groups    []]
    (if (nil? remaining)
      (if current (conj groups [current impls]) groups)
      (let [x (first remaining)]
        (if (seq? x)
          (recur (next remaining) current (conj impls x) groups)
          (if current
            (recur (next remaining) x [] (conj groups [current impls]))
            (recur (next remaining) x [] groups)))))))

(defn ^:private impl-kv-pairs
  "Flatten a list of (m-name [params] body…) forms into an interleaved
  sequence of strings + fn forms, suitable for `(apply hash-map …)`."
  [impls]
  (reduce (fn [acc impl]
            (let [mname (first impl)
                  tail  (rest impl)]
              (conj (conj acc (name mname))
                    (cons 'clojure.core/fn tail))))
          []
          impls))

(defmacro defprotocol
  "Defines a new protocol with the given method signatures. Each sig is
    (method-name [params*] \"doc?\")
  or
    (method-name ([params1*] …) ([params2*] …) \"doc?\")

  A Var named for each method is created in the current namespace,
  dispatching on its first argument's type through the protocol."
  [pname & sigs]
  (let [docstring (when (string? (first sigs)) (first sigs))
        sigs (if docstring (next sigs) sigs)
        method-names (map first sigs)
        method-name-strs (vec (map name method-names))]
    `(do
       (def ~pname
         (clojure.lang.RT/protocol-new
           "user" ~(name pname) ~method-name-strs false))
       ~@(map (fn [m]
                (list 'def m
                  (list 'clojure.lang.RT/protocol-method-new pname (name m))))
              method-names)
       ~pname)))

(defmacro extend-type
  "Extend a type (pyclass, record, or plain Python class) with one or more
  protocol implementations.

    (extend-type T
      P1
      (method1 [this arg] body…)
      (method2 [this] body…)
      P2
      (method3 [this x] body…))"
  [t & specs]
  (let [groups (extend-type-groups specs)]
    `(do
       ~@(map (fn [[proto impls]]
                (let [kvs (impl-kv-pairs impls)]
                  (list 'clojure.lang.RT/protocol-extend-type
                        proto
                        t
                        (cons 'clojure.core/hash-map kvs))))
              groups)
       nil)))

(defmacro extend-protocol
  "Extend one protocol to multiple types.

    (extend-protocol P
      T1
      (method1 [this] …)
      T2
      (method1 [this] …))"
  [proto & specs]
  (let [groups (extend-type-groups specs)]
    `(do
       ~@(map (fn [[t impls]]
                (let [kvs (impl-kv-pairs impls)]
                  (list 'clojure.lang.RT/protocol-extend-type
                        proto
                        t
                        (cons 'clojure.core/hash-map kvs))))
              groups)
       nil)))

(defn satisfies?
  "Returns true if x's type (or an MRO ancestor) implements protocol."
  [protocol x] (clojure.lang.RT/satisfies? protocol x))

;; --- deftype (vanilla 5550-5700) ---
;;
;; Creates a new Python class at runtime via `type(name, (object,), {})`
;; plus a positional constructor `->Name`. Protocol impls are registered
;; on the class via `extend-type` so dispatch goes through the ordinary
;; protocol machinery.

(defmacro deftype
  "(deftype Name [field1 field2 …] Protocol1 (method [this …] body) … ProtocolN …)

  Creates a new type `Name` with the given fields and protocol
  implementations. The positional constructor `->Name` instantiates it:
    (->Name val1 val2 …)
  Fields are stored as Python attributes on instances; access them from
  protocol method bodies via (clojure.lang.RT/getattr this \"fieldname\" nil)
  or by referring to the field symbol directly — each field is captured
  in the method's lexical scope via a `let` at construction time, but for
  runtime field access use getattr."
  [tname fields & specs]
  (let [ctor-sym (symbol (clojure.lang.RT/str-concat "->" (name tname)))
        inst-sym (gensym "inst")
        groups   (extend-type-groups specs)
        setattrs (map (fn [f]
                        (list 'clojure.lang.RT/setattr inst-sym (name f) f))
                      fields)]
    `(do
       (def ~tname (clojure.lang.RT/make-type ~(name tname)))
       (def ~ctor-sym
         (fn ~(vec fields)
           (let [~inst-sym (~tname)]
             ~@setattrs
             ~inst-sym)))
       ~@(map (fn [g]
                (let [proto (first g)
                      impls (second g)
                      kvs   (impl-kv-pairs impls)]
                  (list 'clojure.lang.RT/protocol-extend-type
                        proto
                        tname
                        (cons 'clojure.core/hash-map kvs))))
              groups)
       ~tname)))

;; --- reify (vanilla 6390-6455, simplified) ---
;;
;; Creates an anonymous instance implementing the given protocols. Each
;; call to `reify` creates a fresh class, which is fine for the common
;; case (short-lived callbacks).

(defmacro reify
  "(reify Protocol1 (method [this …] body) … ProtocolN …)

  Returns a new anonymous instance implementing the given protocols.
  Each call to reify creates a fresh class."
  [& specs]
  (let [cname    (gensym "reify_")
        inst-sym (gensym "inst")
        groups   (extend-type-groups specs)]
    `(let [~cname    (clojure.lang.RT/make-type ~(name cname))
           ~inst-sym (~cname)]
       ~@(map (fn [g]
                (let [proto (first g)
                      impls (second g)
                      kvs   (impl-kv-pairs impls)]
                  (list 'clojure.lang.RT/protocol-extend-type
                        proto cname (cons 'clojure.core/hash-map kvs))))
              groups)
       ~inst-sym)))

;; --- defrecord (vanilla 5700-5900, simplified) ---
;;
;; Like `deftype` but additionally:
;;   - Generates a `map->Name` constructor.
;;   - Auto-extends `ILookup` so `(:field record)` and `(get record :field)`
;;     return the value of `field`.
;;   - Auto-extends `IEquiv` / uses identity `__eq__` via our default IEquiv
;;     fallback — structural equality across all fields is a TODO.
;; Users supplying their own `ILookup` impl in `specs` override the default.

(defmacro defrecord
  "Creates a new record type Name with the given fields. Records support
  structural equality (based on type + fields), hashing, seq (of MapEntry
  pairs), and count — like vanilla's defrecord. Unlike deftype, records
  additionally respond to `(:field record)` and `(get record :field)`
  by looking up the field as a Python attribute."
  [rname fields & specs]
  (let [ctor-sym     (symbol (clojure.lang.RT/str-concat "->" (name rname)))
        map-ctor-sym (symbol (clojure.lang.RT/str-concat "map->" (name rname)))
        inst-sym     (gensym "inst")
        other-sym    (gensym "other")
        m-sym        (gensym "m")
        k-sym        (gensym "k")
        nf-sym       (gensym "nf")
        v-sym        (gensym "v")
        groups       (extend-type-groups specs)
        setattrs     (map (fn [f]
                            (list 'clojure.lang.RT/setattr
                                  inst-sym (name f) f))
                          fields)
        map-setattrs (map (fn [f]
                            (list 'clojure.lang.RT/setattr
                                  inst-sym (name f)
                                  (list 'clojure.core/get m-sym
                                        (list 'clojure.core/keyword (name f)))))
                          fields)
        ;; Build a seq of map-entries: `([:a (.-a r)] [:b (.-b r)] ...)`
        entry-forms  (map (fn [f]
                            (list 'clojure.core/vector
                                  (list 'clojure.core/keyword (name f))
                                  (list 'clojure.lang.RT/getattr inst-sym (name f) nil)))
                          fields)
        ;; Field equality expression — `(and (= (.-a a) (.-a b)) ...)`
        equal-fields (map (fn [f]
                            (list 'clojure.core/=
                                  (list 'clojure.lang.RT/getattr inst-sym (name f) nil)
                                  (list 'clojure.lang.RT/getattr other-sym (name f) nil)))
                          fields)]
    `(do
       (def ~rname (clojure.lang.RT/make-type ~(name rname)))
       (def ~ctor-sym
         (fn ~(vec fields)
           (let [~inst-sym (~rname)]
             ~@setattrs
             ~inst-sym)))
       (def ~map-ctor-sym
         (fn [~m-sym]
           (let [~inst-sym (~rname)]
             ~@map-setattrs
             ~inst-sym)))
       ;; ILookup: (:field rec) and (get rec :field).
       (clojure.lang.RT/protocol-extend-type
         clojure._core/ILookup ~rname
         (hash-map "val_at"
                   (fn [~inst-sym ~k-sym ~nf-sym]
                     (clojure.lang.RT/getattr ~inst-sym (name ~k-sym) ~nf-sym))))
       ;; IEquiv: same type + same field values.
       (clojure.lang.RT/protocol-extend-type
         clojure._core/IEquiv ~rname
         (hash-map "equiv"
                   (fn [~inst-sym ~other-sym]
                     (and (clojure.lang.RT/instance? ~rname ~other-sym)
                          ~@equal-fields))))
       ;; Counted: number of fields.
       (clojure.lang.RT/protocol-extend-type
         clojure._core/Counted ~rname
         (hash-map "count" (fn [~inst-sym] ~(count fields))))
       ;; ISeqable: iterate as MapEntry pairs.
       (clojure.lang.RT/protocol-extend-type
         clojure._core/ISeqable ~rname
         (hash-map "seq"
                   (fn [~inst-sym]
                     (clojure.core/seq (clojure.core/list ~@entry-forms)))))
       ;; Associative: assoc known fields keeps record; non-field falls
       ;; back to a plain map. (Vanilla keeps an ext-map on the record;
       ;; we simplify.)
       (clojure.lang.RT/protocol-extend-type
         clojure._core/Associative ~rname
         (hash-map "assoc"
                   (fn [~inst-sym ~k-sym ~v-sym]
                     (let [~m-sym (clojure.core/into {} ~inst-sym)
                           ~m-sym (clojure.core/assoc ~m-sym ~k-sym ~v-sym)
                           field-set# (clojure.core/set
                                        (clojure.core/list
                                          ~@(map (fn [f] (list 'clojure.core/keyword (name f))) fields)))]
                       (if (clojure.core/contains? field-set# ~k-sym)
                         (~map-ctor-sym ~m-sym)
                         ~m-sym)))
                   "contains_key"
                   (fn [~inst-sym ~k-sym]
                     (clojure.core/contains?
                       (clojure.core/set (clojure.core/list ~@(map (fn [f] (list 'clojure.core/keyword (name f))) fields)))
                       ~k-sym))))
       ;; IPersistentCollection: conj on a record follows map's conj
       ;; semantics — for a MapEntry / 2-vec, assoc; for a map, reduce
       ;; assoc. `empty` returns nil (records have no empty instance).
       (clojure.lang.RT/protocol-extend-type
         clojure._core/IPersistentCollection ~rname
         (hash-map "conj"
                   (fn [~inst-sym ~v-sym]
                     (cond
                       (clojure.core/map? ~v-sym)
                         (clojure.core/reduce (fn [acc# e#]
                                                (clojure.core/assoc acc# (clojure.core/key e#) (clojure.core/val e#)))
                                              ~inst-sym
                                              ~v-sym)
                       (clojure.core/vector? ~v-sym)
                         (clojure.core/assoc ~inst-sym (clojure.core/nth ~v-sym 0) (clojure.core/nth ~v-sym 1))
                       :else
                         ;; Assume it's a MapEntry.
                         (clojure.core/assoc ~inst-sym (clojure.core/key ~v-sym) (clojure.core/val ~v-sym))))
                   "empty" (fn [~inst-sym] nil)
                   "count" (fn [~inst-sym] ~(count fields))))
       ;; IPersistentMap: without/dissoc always returns a plain map
       ;; (records can't drop fields).
       (clojure.lang.RT/protocol-extend-type
         clojure._core/IPersistentMap ~rname
         (hash-map "without"
                   (fn [~inst-sym ~k-sym]
                     (clojure.core/dissoc (clojure.core/into {} ~inst-sym) ~k-sym))
                   "assoc"
                   (fn [~inst-sym ~k-sym ~v-sym]
                     (clojure.core/assoc ~inst-sym ~k-sym ~v-sym))))
       ~@(map (fn [g]
                (let [proto (first g)
                      impls (second g)
                      kvs   (impl-kv-pairs impls)]
                  (list 'clojure.lang.RT/protocol-extend-type
                        proto rname (cons 'clojure.core/hash-map kvs))))
              groups)
       ~rname)))

;; --- Sorted collections (vanilla 400-427) ---

(defn sorted-map
  "keyval => key val
  Returns a new sorted map with supplied mappings. If any keys are
  equal, they are handled as if by repeated uses of assoc."
  [& keyvals] (clojure.lang.RT/apply clojure.lang.RT/sorted-map keyvals))

(defn sorted-map-by
  "keyval => key val
  Returns a new sorted map with supplied mappings, using the supplied
  comparator. If any keys are equal, they are handled as if by repeated
  uses of assoc."
  [comparator & keyvals]
  (clojure.lang.RT/apply clojure.lang.RT/sorted-map-by
                         comparator keyvals))

(defn sorted-set
  "Returns a new sorted set with supplied keys. Any equal keys are
  handled as if by repeated uses of conj."
  [& keys] (clojure.lang.RT/apply clojure.lang.RT/sorted-set keys))

(defn sorted-set-by
  "Returns a new sorted set with supplied keys, using the supplied
  comparator. Any equal keys are handled as if by repeated uses of conj."
  [comparator & keys]
  (clojure.lang.RT/apply clojure.lang.RT/sorted-set-by comparator keys))

(defn sorted?
  "Returns true if coll implements Sorted."
  [coll] (clojure.lang.RT/sorted? coll))

(defn rseq
  "Returns, in constant time, a seq of the items in rev (which can be a
  vector or sorted-map), in reverse order. If rev is empty returns nil."
  [rev] (clojure.lang.RT/rseq rev))

(defn subseq
  "sc must be a sorted collection, test(s) one of <, <=, > or >=. Returns
  a seq of those entries with keys ek for which (test (.. sc comparator
  (compare ek key)) 0) is true"
  ([sc test key]
   (let [include (fn* [entry]
                   (let [k (clojure.lang.RT/sorted-entry-key sc entry)
                         r (clojure.lang.RT/compare-values sc k key)]
                     (test r 0)))]
     (if (or (identical? test >) (identical? test >=))
       (let [s (clojure.lang.RT/sorted-seq-from sc key true)]
         (if (and (identical? test >) s (= 0 (clojure.lang.RT/compare-values
                                               sc
                                               (clojure.lang.RT/sorted-entry-key sc (first s))
                                               key)))
           (next s)
           s))
       ;; < or <= : walk ascending, take-while
       (let [s (clojure.lang.RT/sorted-seq sc true)]
         (take-while include s)))))
  ([sc start-test start-key end-test end-key]
   (let [s (clojure.lang.RT/sorted-seq-from sc start-key true)
         s (if (and s (identical? start-test >)
                    (= 0 (clojure.lang.RT/compare-values
                           sc
                           (clojure.lang.RT/sorted-entry-key sc (first s))
                           start-key)))
             (next s)
             s)
         include-end (fn* [entry]
                       (let [k (clojure.lang.RT/sorted-entry-key sc entry)
                             r (clojure.lang.RT/compare-values sc k end-key)]
                         (end-test r 0)))]
     (take-while include-end s))))

(defn rsubseq
  "sc must be a sorted collection, test(s) one of <, <=, > or >=. Returns
  a seq of those entries with keys ek for which (test (.. sc comparator
  (compare ek key)) 0) is true, in descending order."
  ([sc test key]
   (let [include (fn* [entry]
                   (let [k (clojure.lang.RT/sorted-entry-key sc entry)
                         r (clojure.lang.RT/compare-values sc k key)]
                     (test r 0)))]
     (if (or (identical? test <) (identical? test <=))
       (let [s (clojure.lang.RT/sorted-seq-from sc key false)]
         (if (and (identical? test <) s
                  (= 0 (clojure.lang.RT/compare-values
                         sc
                         (clojure.lang.RT/sorted-entry-key sc (first s))
                         key)))
           (next s)
           s))
       (let [s (clojure.lang.RT/sorted-seq sc false)]
         (take-while include s)))))
  ([sc start-test start-key end-test end-key]
   (let [s (clojure.lang.RT/sorted-seq-from sc end-key false)
         s (if (and s (identical? end-test <)
                    (= 0 (clojure.lang.RT/compare-values
                           sc
                           (clojure.lang.RT/sorted-entry-key sc (first s))
                           end-key)))
             (next s)
             s)
         include-start (fn* [entry]
                         (let [k (clojure.lang.RT/sorted-entry-key sc entry)
                               r (clojure.lang.RT/compare-values sc k start-key)]
                           (start-test r 0)))]
     (take-while include-start s))))

;; --- Agents (vanilla 2075-2275) ---
;; Independent state type updated asynchronously via `send`/`send-off`. See
;; src/agent.rs for the runtime (executor pools + per-agent FIFO queue).

(def ^:dynamic *agent* nil)

(defn agent
  "Creates and returns an agent with an initial value of state and zero or
  more options (in any order):
    :meta metadata-map  :validator validate-fn
    :error-handler handler-fn  :error-mode mode-keyword
  If metadata-map is supplied, it will become the metadata on the agent.
  validate-fn must be nil or a side-effect-free fn of one argument; it will
  be passed the intended new state on any state change. If the new state is
  unacceptable, the validate-fn should return false or throw an exception.
  handler-fn is called if an action throws an exception or if validate-fn
  rejects a new state -- see set-error-handler! for details. The mode
  keyword may be either :continue (the default if an error-handler is given)
  or :fail (the default if none)."
  [state & options]
  (clojure.lang.RT/agent-new state options))

(defn send
  "Dispatch an action to an agent. Returns the agent immediately.
  Subsequently, in a thread from a thread pool, the state of the agent will
  be set to the value of:  (apply action-fn state-of-agent args)"
  [a f & args] (clojure.lang.RT/agent-send a f args))

(defn send-off
  "Dispatch a potentially blocking action to an agent. Returns the agent
  immediately. Subsequently, in a separate thread, the state of the agent
  will be set to the value of:  (apply action-fn state-of-agent args)"
  [a f & args] (clojure.lang.RT/agent-send-off a f args))

(defn send-via
  "Dispatch an action to an agent via the supplied executor. Returns the
  agent immediately."
  [executor a f & args] (clojure.lang.RT/agent-send-via executor a f args))

(defn release-pending-sends
  "Normally, actions sent directly or indirectly during another action are
  held until the action completes (changes the agent's state). This function
  can be used to dispatch any pending sent actions immediately. This has no
  impact on actions sent during a transaction, which are still held until
  commit. If no action is occurring, does nothing. Returns the number of
  actions dispatched."
  [] (clojure.lang.RT/agent-release-pending))

(defn await
  "Blocks the current thread (indefinitely!) until all actions dispatched
  thus far, from this thread or agent, to the agent(s) have occurred. Will
  block on failed agents. Will never return if a failed agent is restarted
  with :clear-actions true."
  [& agents] (clojure.lang.RT/agent-await agents))

(defn await-for
  "Blocks the current thread until all actions dispatched thus far (from
  this thread or agent) to the agents have occurred, or the timeout (in
  milliseconds) has elapsed. Returns logical false if returning due to
  timeout, logical true otherwise."
  [timeout-ms & agents] (clojure.lang.RT/agent-await-for timeout-ms agents))

(defn agent-error
  "Returns the exception thrown during an asynchronous action of the agent
  if the agent is failed. Returns nil if the agent is not failed."
  [a] (clojure.lang.RT/agent-error a))

(defn restart-agent
  "When an agent is failed, changes the agent state to new-state and then
  un-fails the agent so that sends are allowed again. If a :clear-actions
  true option is given, any actions queued on the agent that were being
  held while it was failed will be discarded, otherwise those held actions
  will proceed. The new-state must pass the validator if any, or restart
  will throw an exception and the agent will remain failed with its old
  state and error. Watchers, if any, will NOT be notified of the new state.
  Throws an exception if the agent is not failed."
  [a new-state & options]
  (clojure.lang.RT/agent-restart a new-state options))

(defn set-error-handler!
  "Sets the error-handler of agent a to handler-fn. If an action being run
  by the agent throws an exception or doesn't pass the validator fn,
  handler-fn will be called with two arguments: the agent and the exception."
  [a handler-fn] (clojure.lang.RT/agent-set-error-handler a handler-fn))

(defn error-handler
  "Returns the error-handler of agent a, or nil if there is none."
  [a] (clojure.lang.RT/agent-get-error-handler a))

(defn set-error-mode!
  "Sets the error-mode of agent a to mode-keyword, which must be either
  :fail or :continue. If an action being run by the agent throws an
  exception or doesn't pass the validator fn, an error-handler may be
  called (see set-error-handler!), after which, if the mode is :continue,
  the agent will continue as if neither the action that caused the error
  nor the error itself ever happened. If the mode is :fail the agent will
  become failed and will stop accepting new 'send' and 'send-off' actions,
  and any previously queued actions will be held until a 'restart-agent'."
  [a mode-keyword] (clojure.lang.RT/agent-set-error-mode a mode-keyword))

(defn error-mode
  "Returns the error-mode of agent a.  See set-error-mode!"
  [a] (clojure.lang.RT/agent-get-error-mode a))

(defn agent-errors
  "DEPRECATED: Use 'agent-error' instead. Returns a sequence of the
  exceptions thrown during asynchronous actions of the agent."
  [_a] nil)

(defn clear-agent-errors
  "DEPRECATED: Use 'restart-agent' instead. Clears any exceptions thrown
  during asynchronous actions of the agent, allowing subsequent actions to
  occur."
  [a] (clojure.lang.RT/agent-clear-errors a))

(defn shutdown-agents
  "Initiates a shutdown of the thread pools that back the agent system.
  Running actions will complete, but no new actions will be accepted."
  [] (clojure.lang.RT/agent-shutdown))

;; --- Refs and STM (vanilla 2283-2533) ---
;; Coordinated, synchronous, MVCC-backed state. See src/stm/ for the runtime
;; (Ref + LockingTransaction). Most of this block lands incrementally — the
;; transaction primitives (ref-set, alter, commute, ensure, sync/dosync, io!)
;; depend on the LockingTransaction machinery.

(defn ref
  "Creates and returns a Ref with an initial value of x and zero or more
  options (in any order):
    :meta metadata-map  :validator validate-fn
    :min-history (default 0)  :max-history (default 10)
  If metadata-map is supplied, it will become the metadata on the ref.
  validate-fn must be nil or a side-effect-free fn of one argument."
  ([x] (clojure.lang.RT/ref-new x))
  ([x & options]
   (clojure.lang.RT/apply clojure.lang.RT/ref-new x options)))

(defn ref-set
  "Must be called in a transaction. Sets the value of ref. Returns val."
  [r val] (clojure.lang.RT/ref-set-bang r val))

(defn alter
  "Must be called in a transaction. Sets the in-transaction-value of ref to:
  (apply fun in-transaction-value-of-ref args)  Returns the in-transaction-value of ref."
  [r fun & args] (clojure.lang.RT/ref-alter r fun args))

(defn commute
  "Must be called in a transaction. Sets the in-transaction-value of ref to:
  (apply fun in-transaction-value-of-ref args). At commit point of the
  transaction, sets the value of ref to:
  (apply fun most-recently-committed-value-of-ref args). Thus fun should be
  commutative, or, failing that, you must accept last-one-in-wins behavior.
  commute allows for more concurrency than ref-set."
  [r fun & args] (clojure.lang.RT/ref-commute r fun args))

(defn ensure
  "Must be called in a transaction. Protects the ref from modification by other
  transactions. Returns the in-transaction-value of ref. Allows for more
  concurrency than (ref-set ref @ref)."
  [r] (clojure.lang.RT/ref-ensure r))

(defmacro sync
  "transaction-flags => TBD, pass nil for now. Runs the exprs (in an implicit
  do) in a transaction that encompasses exprs and any nested calls. Starts a
  transaction if none is already running on this thread. Any uncaught
  exception will abort the transaction and flow out of sync. The exprs may be
  run more than once, but any effects on Refs will be atomic."
  [flags-ignored & body]
  `(clojure.lang.RT/ref-run-in-txn (fn* [] ~@body)))

(defmacro dosync
  "Runs the exprs (in an implicit do) in a transaction that encompasses exprs
  and any nested calls. Starts a transaction if none is already running on
  this thread. Any uncaught exception will abort the transaction and flow out
  of dosync. The exprs may be run more than once, but any effects on Refs
  will be atomic."
  [& exprs]
  `(sync nil ~@exprs))

(defmacro io!
  "If an io! block occurs in a transaction, throws IllegalStateException, else
  runs body in an implicit do. If the first expression in body is a literal
  string, will use that as the exception message."
  [& body]
  `(do (clojure.lang.RT/io-bang-check) ~@body))

(defn ref-history-count
  "Returns the history count of a ref."
  [r] (clojure.lang.RT/ref-history-count r))

(defn ref-min-history
  "Gets the min-history of a ref, or sets it and returns the ref"
  ([r] (clojure.lang.RT/ref-min-history r))
  ([r n] (clojure.lang.RT/ref-min-history r n)))

(defn ref-max-history
  "Gets the max-history of a ref, or sets it and returns the ref"
  ([r] (clojure.lang.RT/ref-max-history r))
  ([r n] (clojure.lang.RT/ref-max-history r n)))

;; --- Printing (vanilla 3691-3826) ---
;; `*out*` is the writer that `pr` / `prn` / `print` / `println` emit to.
;; Defaults to `sys.stdout`; override via `(binding [*out* writer] ...)`.
;;
;; Print dispatch flows through the `print-method` multimethod, keyed on
;; `(type x)`. Collection methods iterate and re-call `print-method` on
;; each element, so user extensions for element types flow through nested
;; data. The `:default` method delegates to Rust's `pr_str`/`print_str`
;; (fast path; handles every built-in currently ported).

(def ^:dynamic *out* (clojure.lang.RT/py-sys-stdout))
(def ^:dynamic *err* (clojure.lang.RT/py-sys-stderr))
(def ^:dynamic *in*  (clojure.lang.RT/py-sys-stdin))

(def ^:dynamic *print-readably*
  "When true (default), `pr` / `prn` emit strings quoted (re-readable).
  `print` / `println` rebind this to false for human-readable output."
  true)

(def ^:dynamic *print-dup*
  "When true, `pr-on` dispatches via `print-dup` instead of `print-method`."
  false)

(defmulti print-method
  "Multimethod for printing the textual representation of any value to a
  writer. Dispatches on `(type x)`. Users may extend for their own types:
    (defmethod print-method MyType [x w] (.write w \"<mine>\"))
  Collection methods call back through `print-method` for elements, so
  user extensions flow through nested collections."
  (fn [x _w] (type x)))

(defmulti print-dup
  "Multimethod for the re-readable textual representation of any value.
  Used when `*print-dup*` is true. Defaults to `print-method`."
  (fn [x _w] (type x)))

(defmethod print-method :default [x w]
  (clojure.lang.RT/writer-write w
    (if *print-readably*
      (clojure.lang.RT/pr-str x)
      (clojure.lang.RT/print-str x)))
  nil)

(defmethod print-dup :default [x w]
  (print-method x w))

;; --- Collection methods (iterate + recurse through print-method) ---

(defn ^:private print-sequential [open close sep coll w]
  (clojure.lang.RT/writer-write w open)
  (loop [items (seq coll)]
    (when items
      (print-method (first items) w)
      (when-let [r (next items)]
        (clojure.lang.RT/writer-write w sep)
        (recur r))))
  (clojure.lang.RT/writer-write w close))

(defn ^:private print-map [m w]
  (clojure.lang.RT/writer-write w "{")
  (loop [entries (seq m)]
    (when entries
      (let [e (first entries)]
        (print-method (key e) w)
        (clojure.lang.RT/writer-write w " ")
        (print-method (val e) w))
      (when-let [r (next entries)]
        (clojure.lang.RT/writer-write w ", ")
        (recur r))))
  (clojure.lang.RT/writer-write w "}"))

(defmethod print-method clojure._core/PersistentVector [v w]
  (print-sequential "[" "]" " " v w))

(defmethod print-method clojure._core/PersistentList [l w]
  (print-sequential "(" ")" " " l w))

(defmethod print-method clojure._core/EmptyList [_ w]
  (clojure.lang.RT/writer-write w "()"))

(defmethod print-method clojure._core/PersistentHashMap [m w]
  (print-map m w))

(defmethod print-method clojure._core/PersistentArrayMap [m w]
  (print-map m w))

(defmethod print-method clojure._core/PersistentTreeMap [m w]
  (print-map m w))

(defmethod print-method clojure._core/PersistentHashSet [s w]
  (print-sequential "#{" "}" " " s w))

(defmethod print-method clojure._core/PersistentTreeSet [s w]
  (print-sequential "#{" "}" " " s w))

(defmethod print-method clojure._core/Cons [s w]
  (print-sequential "(" ")" " " s w))

(defmethod print-method clojure._core/LazySeq [s w]
  (print-sequential "(" ")" " " s w))

(defmethod print-method clojure._core/VectorSeq [s w]
  (print-sequential "(" ")" " " s w))

;; --- Reference-type methods: print wrapper + recurse into state ---

(defmethod print-method clojure._core/Atom [a w]
  (clojure.lang.RT/writer-write w "#<Atom ")
  (print-method @a w)
  (clojure.lang.RT/writer-write w ">"))

(defmethod print-method clojure._core/Volatile [v w]
  (clojure.lang.RT/writer-write w "#<Volatile ")
  (print-method @v w)
  (clojure.lang.RT/writer-write w ">"))

(defmethod print-method clojure._core/Ref [r w]
  (clojure.lang.RT/writer-write w "#<Ref ")
  (print-method @r w)
  (clojure.lang.RT/writer-write w ">"))

(defmethod print-method clojure._core/Agent [a w]
  (clojure.lang.RT/writer-write w "#<Agent ")
  (print-method @a w)
  (clojure.lang.RT/writer-write w ">"))

;; --- Public API -----------------------------------------------------------

(defn pr-str
  "pr to a string, returning it."
  ([] "")
  ([x]
   (let [buf (clojure.lang.RT/make-string-writer)]
     (print-method x buf)
     (clojure.lang.RT/string-writer-value buf)))
  ([x & more]
   (apply str (pr-str x)
          (mapcat (fn* [m] [" " (pr-str m)]) more))))

(defn print-str
  "print to a string, returning it."
  ([] "")
  ([x]
   (binding [*print-readably* false]
     (pr-str x)))
  ([x & more]
   (apply str (print-str x)
          (mapcat (fn* [m] [" " (print-str m)]) more))))

(defn pr-on
  "Low-level: dispatch to `print-method` (or `print-dup` if `*print-dup*`)
  on x, writing to w."
  [x w]
  (if *print-dup*
    (print-dup x w)
    (print-method x w))
  nil)

(defn newline
  "Writes a platform-specific newline to *out*."
  []
  (clojure.lang.RT/writer-write *out* "\n")
  nil)

(defn flush
  "Flushes the output stream that is the current value of *out*."
  []
  (clojure.lang.RT/writer-flush *out*)
  nil)

(defn pr
  "Prints the object(s) to the output stream that is the current value of
  *out*. Prints the object(s), separated by spaces if there is more than
  one. By default, pr and prn print in a way that objects can be read by
  the reader."
  ([] nil)
  ([x] (pr-on x *out*))
  ([x & more]
   (pr x)
   (doseq [m more]
     (clojure.lang.RT/writer-write *out* " ")
     (pr m))))

(defn prn
  "Same as pr followed by (newline). Observes *flush-on-newline*."
  [& more]
  (apply pr more)
  (newline))

(defn print
  "Prints the object(s) to the output stream that is the current value of
  *out*. Print and println produce output for human consumption."
  [& more]
  (binding [*print-readably* false]
    (apply pr more)))

(defn println
  "Same as print followed by (newline)."
  [& more]
  (apply print more)
  (newline))

(defn prn-str
  "Same as pr-str followed by (newline)."
  [& xs] (str (apply pr-str xs) "\n"))

(defn println-str
  "print-str followed by (newline)."
  [& xs] (str (apply print-str xs) "\n"))

;; --- I/O (vanilla 3771-3835, partial) ---

(defn read-line
  "Reads the next line of text from *in*. Returns nil at EOF."
  [] (clojure.lang.RT/reader-readline *in*))

(defn load-string
  "Sequentially reads and evaluates the set of forms contained in the
  string. Forms are evaluated in the clojure.user namespace."
  [s] (clojure.lang.RT/load-string s))

(defn load-reader
  "Sequentially reads and evaluates the set of forms from the Python
  reader. Forms are evaluated in the clojure.user namespace."
  [rdr]
  (load-string (.read rdr)))

(defn line-seq
  "Returns the lines of text from rdr as a lazy sequence of strings. rdr
  must be a Python file-like object with a readline method."
  [rdr]
  (lazy-seq
    (let [line (clojure.lang.RT/reader-readline rdr)]
      (if (nil? line)
        nil
        (cons line (line-seq rdr))))))

;; --- Pushback-aware read -------------------------------------------------
;;
;; `read` reads exactly one form from a reader and leaves any unconsumed
;; text stashed on the reader as a `__clj_pushback__` attribute for the
;; next call. Multi-line forms are handled by reading additional lines
;; whenever the parser signals "EOF while reading ..." on an incomplete
;; buffer.
;;
;; Falls back gracefully when the reader object can't have attributes
;; stored on it (e.g. some C-implemented streams) — we still parse, but
;; unconsumed bytes are discarded. In practice this only matters for
;; concatenated forms on one line; the common REPL case of one-form-per-
;; read works regardless.

(defn ^:private pushback-buffer [rdr]
  (if (clojure.lang.RT/hasattr rdr "__clj_pushback__")
    (or (clojure.lang.RT/getattr rdr "__clj_pushback__" nil) "")
    ""))

(defn ^:private set-pushback-buffer! [rdr buf]
  ;; Attempt setattr. If the reader is immutable (rare — some C-implemented
  ;; streams) the error bubbles up and the caller sees an AttributeError;
  ;; callers who want that robust should wrap `*in*` first.
  (clojure.lang.RT/setattr rdr "__clj_pushback__" buf))

(defn ^:private eof-error? [e]
  (let [msg (str e)]
    (or (clojure.lang.RT/str-contains? msg "EOF while reading")
        (clojure.lang.RT/str-contains? msg "EOF after"))))

(defn read
  "Reads the next object from a reader (default *in*). Accepts multi-line
  forms by accumulating lines until the parser accepts a complete form.
  Unconsumed trailing bytes are stashed on the reader for the next call.
  Returns nil at EOF."
  ([] (read *in*))
  ([rdr]
   (loop [buf (pushback-buffer rdr)]
     ;; If buffer is empty, read a line; if readline returns nil we're at EOF.
     (let [buf (if (= "" buf)
                 (let [line (clojure.lang.RT/reader-readline rdr)]
                   (if (nil? line) nil (str line "\n")))
                 buf)]
       (cond
         (nil? buf) nil
         (= "" buf) nil
         :else
         (let [result (try
                        (clojure.lang.RT/read-string-prefix buf)
                        (catch clojure._core/ReaderError e
                          (if (eof-error? e) ::more (throw e))))]
           (if (identical? result ::more)
             (let [more-line (clojure.lang.RT/reader-readline rdr)]
               (if (nil? more-line)
                 ;; Stream ended mid-form — propagate as error.
                 (clojure.lang.RT/throw-ise
                   (clojure.lang.RT/str-concat "EOF while reading, partial: " buf))
                 (recur (str buf more-line "\n"))))
             (let [form (nth result 0)
                   consumed (nth result 1)]
               (set-pushback-buffer! rdr (clojure.lang.RT/subs buf consumed))
               form))))))))

;; --- NS introspection (vanilla 4146-4311) ---

(defn all-ns
  "Returns a sequence of all namespaces."
  [] (clojure.lang.RT/all-ns))

(defn remove-ns
  "Removes the namespace named by the symbol. Use with caution.
  Cannot be used to remove the clojure.core namespace."
  [sym] (clojure.lang.RT/remove-ns sym))

(defn ns-map
  "Returns a map of all the mappings for the namespace."
  [ns] (clojure.lang.RT/ns-map (the-ns ns)))

(defn ns-publics
  "Returns a map of the public intern mappings for the namespace."
  [ns] (clojure.lang.RT/ns-publics (the-ns ns)))

(defn ns-interns
  "Returns a map of the intern mappings for the namespace."
  [ns] (clojure.lang.RT/ns-interns (the-ns ns)))

(defn ns-unmap
  "Removes the mapping for the symbol from the namespace."
  [ns sym] (clojure.lang.RT/ns-unmap (the-ns ns) sym))

(defn ns-refers
  "Returns a map of the refer mappings for the namespace."
  [ns] (clojure.lang.RT/ns-refers (the-ns ns)))

(defn ns-aliases
  "Returns a map of the aliases for the namespace."
  [ns] (clojure.lang.RT/ns-aliases (the-ns ns)))

(defn alias
  "Add an alias in the current namespace to another namespace. Arguments
  are two symbols: the alias to be used, the symbolic name of the target
  namespace."
  [alias-sym target-sym]
  (let [target (or (find-ns target-sym)
                   (throw (clojure.lang.RT/throw-ise
                            (str "No namespace: " target-sym))))]
    (clojure.lang.RT/alias *ns* alias-sym target)))

(defn ns-unalias
  "Removes the alias for the symbol from the namespace."
  [ns sym] (clojure.lang.RT/ns-unalias (the-ns ns) sym))

(defn refer
  "refers to all public vars of ns, subject to filters. filters can include
  at most one each of :exclude, :only, :rename. Full vanilla filter
  semantics are implemented here."
  [ns-sym & filters]
  (let [target (or (find-ns ns-sym)
                   (throw (clojure.lang.RT/throw-ise
                            (str "No namespace: " ns-sym))))
        opts   (apply hash-map filters)
        exclude (set (:exclude opts))
        only    (when (contains? opts :only) (set (:only opts)))
        rename  (or (:rename opts) {})
        pubs    (ns-publics target)
        current *ns*]
    (doseq [[sym var] pubs]
      (when (and (not (contains? exclude sym))
                 (or (nil? only) (contains? only sym)))
        (let [rsym (get rename sym sym)]
          (clojure.lang.RT/refer current rsym var))))
    nil))

;; --- Loading / require / use / ns (vanilla 5800-6300) ---

(def ^:dynamic *loaded-libs* (atom #{}))

(defn in-ns
  "Sets `*ns*` to the namespace named by sym, creating it if needed.
  When called inside a load loop (file or load-string), subsequent forms
  evaluate into that namespace. Returns the namespace."
  [sym] (clojure.lang.RT/in-ns sym))

(defn load-file
  "Sequentially read and evaluate the set of forms contained in the file
  at the given path. Returns the value of the last form."
  [path]
  (let [src (clojure.lang.RT/read-file-text path)]
    (clojure.lang.RT/load-string src)))

(defn ^:private require-spec
  "Process one require / use spec. `spec` is either a Symbol (just load)
  or a Vector [ns-sym & {:as a :refer [...] :only [...] :exclude [...]
  :rename {...}}]. `use?` controls whether to refer all public vars by
  default. Returns nil; side-effects load the file (if needed), record in
  *loaded-libs*, install alias / refer mappings."
  [spec use?]
  (let [vec? (vector? spec)
        ns-sym (if vec? (first spec) spec)
        opts (if vec? (apply hash-map (rest spec)) {})
        already? (contains? @*loaded-libs* ns-sym)
        reload? (or (:reload opts) (:reload-all opts))]
    (when (or reload? (not already?))
      ;; Load if not already loaded.
      (let [path (clojure.lang.RT/find-source-file ns-sym)]
        (when (nil? path)
          (clojure.lang.RT/throw-iae
            (clojure.lang.RT/str-concat "Could not locate source for: " (str ns-sym))))
        (load-file path))
      (swap! *loaded-libs* conj ns-sym))
    ;; Apply :as alias.
    (when-let [as-sym (:as opts)]
      (let [target (or (find-ns ns-sym)
                       (clojure.lang.RT/throw-ise
                         (clojure.lang.RT/str-concat "ns not found: " (str ns-sym))))]
        (clojure.lang.RT/alias *ns* as-sym target)))
    ;; Apply :refer / :use semantics.
    (let [refer-opts (cond
                       (:refer opts) opts
                       (and use? (not (:refer opts)))
                       (assoc opts :refer :all)
                       :else nil)]
      (when refer-opts
        (let [r (:refer refer-opts)
              ;; `extra` is a flat keyword/value sequence of additional
              ;; refer args. For :all we add nothing; for an explicit list
              ;; we add `:only [...names...]`.
              extra (cond
                      (= r :all) []
                      (sequential? r) [:only (vec r)]
                      :else [])
              exclude (when (:exclude refer-opts) [:exclude (:exclude refer-opts)])
              rename  (when (:rename  refer-opts) [:rename  (:rename refer-opts)])]
          (apply refer ns-sym (concat extra exclude rename)))))
    nil))

(defn require
  "Loads namespaces, with options.

    (require 'foo.bar)
    (require '[foo.bar :as bar])
    (require '[foo.bar :refer [baz quux]])
    (require '[foo.bar :as bar :refer [baz]])

  Multiple specs may be given. Bare-symbol forms are treated as quoted."
  [& specs]
  (doseq [spec specs]
    (require-spec spec false)))

(defn use
  "Like require, but also refers all public vars from the loaded namespace
  (subject to :only / :exclude / :rename filters) into the current namespace."
  [& specs]
  (doseq [spec specs]
    (require-spec spec true)))

(defmacro ns
  "Sets `*ns*` to the namespace named by name, creating it if needed.
  `references` may include
    (:require [namespace ...] ...)
    (:use     [namespace ...] ...)

  Auto-refers `clojure.core` so the standard library is available."
  [name & references]
  (let [ns-name (str name)
        directive-forms
        (mapcat (fn [ref-form]
                  (let [k (first ref-form)
                        specs (rest ref-form)]
                    (cond
                      (= k :require)
                      (map (fn [s]
                             (list 'clojure.core/require (list 'quote s)))
                           specs)
                      (= k :use)
                      (map (fn [s]
                             (list 'clojure.core/use (list 'quote s)))
                           specs)
                      :else
                      ;; :import etc. — best-effort no-op for now.
                      nil)))
                references)]
    `(do
       (clojure.lang.RT/in-ns (clojure.core/symbol ~ns-name))
       ;; Refer clojure.core into the new namespace so basic fns are
       ;; available without qualification. Best-effort: skip if
       ;; clojure.core hasn't been registered (e.g. during init).
       (let [target# (find-ns (clojure.core/symbol ~ns-name))
             core#   (find-ns (clojure.core/symbol "clojure.core"))]
         (when core#
           (doseq [[s# v#] (ns-publics core#)]
             (clojure.lang.RT/refer target# s# v#))))
       ~@directive-forms
       nil)))

;; --- Futures / promises / pmap / with-open (vanilla 6800-7100) ---

(defn future?
  "Returns true if x is a future."
  [x] (clojure.lang.RT/future? x))

(defn future-call
  "Takes a function of no args and yields a future object that will invoke
  the function in a thread, and will cache the result and return it on all
  subsequent calls to deref/@. If the computation has not yet finished,
  calls to deref/@ will block, unless the variant of deref with timeout is
  used."
  [f] (clojure.lang.RT/future-call f))

(defmacro future
  "Takes a body of expressions and yields a future object that will invoke
  the body in another thread, and will cache the result and return it on
  all subsequent calls to deref/@. If the computation has not yet finished,
  calls to deref/@ will block, unless the variant of deref with timeout is
  used."
  [& body]
  `(future-call (fn [] ~@body)))

(defn future-cancel
  "Best-effort: cancels future if it has not yet started. Python provides
  no cooperative thread interrupt, so an in-flight computation continues
  to run; its result is discarded once cancellation is observed. Returns
  true if cancellation was applied (future was still pending)."
  [f] (clojure.lang.RT/future-cancel f))

(defn future-cancelled?
  "Returns true if future f has been cancelled."
  [f] (clojure.lang.RT/future-cancelled? f))

(defn future-done?
  "Returns true if future f is done."
  [f] (clojure.lang.RT/future-done? f))

(defn promise
  "Returns a promise object that can be read with deref/@, and set, once
  only, with deliver. Calls to deref/@ prior to delivery will block."
  [] (clojure.lang.RT/promise))

(defn deliver
  "Delivers the supplied value to the promise, releasing any pending
  derefs. A subsequent call to deliver on a promise will have no effect."
  [promise val] (clojure.lang.RT/deliver promise val))

(defn realized?
  "Returns true if a value has been produced for a promise, future, or
  delay; false for any other value (including a value that doesn't
  implement IPending)."
  [x] (clojure.lang.RT/realized? x))

(defn pmap
  "Like map, except f is applied in parallel. Semi-lazy in that the
  parallel computation stays ahead of the consumption, but doesn't realize
  the entire result unless required. Only useful for computationally
  intensive functions where the time of f dominates the coordination
  overhead."
  ([f coll]
   (let [n (+ 2 (clojure.lang.RT/available-parallelism))
         rets (map (fn [x] (future-call (fn [] (f x)))) coll)
         step (fn step [vs fs]
                (lazy-seq
                  (if-let [s (seq fs)]
                    (cons (deref (first vs)) (step (rest vs) (rest s)))
                    (map deref vs))))]
     (step rets (drop n rets))))
  ([f coll & colls]
   (let [step (fn step [cs]
                (lazy-seq
                  (let [ss (map seq cs)]
                    (when (every? identity ss)
                      (cons (map first ss) (step (map rest ss)))))))]
     (pmap (fn [args] (apply f args)) (step (cons coll colls))))))

(defn pcalls
  "Executes the no-arg fns in parallel, returning a lazy sequence of their
  values."
  [& fns] (pmap (fn [f] (f)) fns))

(defmacro pvalues
  "Returns a lazy sequence of the values of the exprs, which are evaluated
  in parallel."
  [& exprs]
  `(pcalls ~@(map (fn [e] `(fn [] ~e)) exprs)))

;; --- with-open + extenders / extends? ---

(defmacro with-open
  "bindings => [name init …]

  Evaluates body in a try expression with names bound to the values of
  the inits, and a finally clause that calls (close-resource name) on
  each name in reverse order. close-resource invokes the Python
  context-manager `__exit__` if available, falling back to `.close()`."
  [bindings & body]
  (assert (vector? bindings) "with-open requires a vector for its bindings")
  (assert (zero? (mod (count bindings) 2))
          "with-open requires an even number of forms in binding vector")
  (cond
    (= (count bindings) 0)
    `(do ~@body)
    :else
    (let [name (bindings 0)
          init (bindings 1)
          rest-bindings (vec (drop 2 bindings))]
      `(let [~name ~init]
         (try
           (with-open ~rest-bindings ~@body)
           (finally
             (clojure.lang.RT/close-resource ~name)))))))

(defn extenders
  "Returns a sequence of types that have direct (non-MRO-promoted)
  implementations registered for the given protocol."
  [protocol] (clojure.lang.RT/extenders protocol))

(defn extends?
  "Returns true if the protocol is implemented for the given type
  (or value's type) — directly or via MRO inheritance."
  [protocol x] (clojure.lang.RT/extends? protocol x))

(defn bean
  "Takes a Python object and returns a snapshot map of its public,
  non-callable attributes, keyed by keyword. Methods and dunder names
  are excluded. `@property`-decorated attributes appear with their
  getter values. The result is an ordinary persistent map; mutations to
  the underlying object after `bean` is called are NOT reflected."
  [obj] (clojure.lang.RT/bean-impl obj))

;; --- Java arrays analogue (vanilla 3928-4048) ---
;; Python has no typed-primitive-array type, so the runtime representation is
;; a Python `list` for everything. `aset` mutates; `aget` reads. Typed
;; variants (aset-int, aset-long, aset-byte, etc.) all alias `aset` — Python
;; ints/floats are already unbounded, so the Java width distinctions collapse.
;; `aset-char` coerces to a 1-char string; `aset-boolean` to Python bool.

(defn make-array
  "Creates and returns an array of the specified dimensions. Note: the type
  argument is accepted for Clojure-code portability but ignored — Python
  lists are heterogeneous.
  Accepts either (make-array dim) or (make-array type dim1 dim2 ...)."
  [& args] (clojure.lang.RT/apply clojure.lang.RT/array-make args))

;; (typed-array fns are below, after aset)

(defn aget
  "Returns the value at the index/indices. Works on any Python sequence."
  ([array idx] (clojure.lang.RT/array-aget array idx))
  ([array idx & idxs]
   (clojure.lang.RT/apply clojure.lang.RT/array-aget array idx idxs)))

(defn aset
  "Sets the value at the index/indices. Works on any Python mutable
  sequence. Returns val."
  ([array idx val] (clojure.lang.RT/array-aset array idx val))
  ([array idx idx2 & rest]
   (clojure.lang.RT/apply clojure.lang.RT/array-aset array idx idx2 rest)))

(defn aclone
  "Returns a clone (shallow copy) of the Java array (Python list). The
  returned list is always a list, regardless of the input container type."
  [array] (clojure.lang.RT/array-aclone array))

(defn alength
  "Returns the length of the Python sequence (equivalent to (count a) for a
  list/tuple)."
  [array] (clojure.lang.RT/array-alength array))

(defn to-array
  "Returns a (Python) list containing the contents of coll. Accepts any
  ISeqable."
  [coll] (clojure.lang.RT/array-to-array coll))

(defn into-array
  "Returns a list representing the contents of coll. The type argument is
  accepted for portability but ignored.
  Accepts either (into-array aseq) or (into-array type aseq)."
  ([aseq] (clojure.lang.RT/array-into-array aseq))
  ([_type aseq] (clojure.lang.RT/array-into-array aseq)))

(defn to-array-2d
  "Returns a (Python) list of lists containing the contents of coll. Each
  inner seq is converted element-wise."
  [coll] (clojure.lang.RT/array-to-array-2d coll))

;; Typed aset variants — Python has no type-width distinction. All alias to
;; (aset) directly. `aset-char` / `aset-boolean` apply a best-effort coercion.

(defn aset-int    [array idx val] (aset array idx (int val)))
(defn aset-long   [array idx val] (aset array idx (int val)))
(defn aset-short  [array idx val] (aset array idx (int val)))
(defn aset-byte   [array idx val] (aset array idx (int val)))
(defn aset-float  [array idx val] (aset array idx (float val)))
(defn aset-double [array idx val] (aset array idx (float val)))
(defn aset-char   [array idx val] (aset array idx val))
(defn aset-boolean [array idx val] (aset array idx (boolean val)))

(defmacro amap
  "Maps an expression across an array a, using an index named idx, and
  return value named ret, initialized to a clone of a, then setting each
  element of ret to the evaluation of expr, returning the new array ret."
  [a idx ret expr]
  `(let [a# ~a
         ~ret (aclone a#)]
     (loop [~idx 0]
       (if (< ~idx (alength a#))
         (do (aset ~ret ~idx ~expr)
             (recur (inc ~idx)))
         ~ret))))

(defmacro areduce
  "Reduces an expression across an array a, using an index named idx, and
  return value named ret, initialized to init, setting ret to the evaluation
  of expr at each step, returning ret."
  [a idx ret init expr]
  `(let [a# ~a]
     (loop [~idx 0 ~ret ~init]
       (if (< ~idx (alength a#))
         (recur (inc ~idx) ~expr)
         ~ret))))

;; --- Typed Java arrays (after make-array/aset) ---------------------------

(defn boolean-array
  "Creates an array of booleans (here: a Python list)."
  ([size-or-seq]
   (if (number? size-or-seq)
     (make-array nil size-or-seq)
     (vec size-or-seq)))
  ([size init]
   (let [a (make-array nil size)]
     (dotimes [i size] (aset a i init))
     a)))

(def byte-array    boolean-array)
(def char-array    boolean-array)
(def short-array   boolean-array)
(def int-array     boolean-array)
(def long-array    boolean-array)
(def float-array   boolean-array)
(def double-array  boolean-array)
(def object-array  boolean-array)

;; ============================================================================
;; Phase-3 alignment: forms that depend on later-defined names (printf
;; needs print, intern needs the-ns, requiring-resolve needs require, …).
;; ============================================================================

(defn printf
  "Prints formatted output, as per format."
  [fmt & args]
  (print (apply format fmt args)))

(defmacro with-out-str
  "Evaluates exprs in a context in which *out* is bound to a fresh
  StringIO. Returns the string created."
  [& body]
  `(let [s# (clojure.lang.RT/string-io)]
     (binding [*out* s#]
       ~@body
       (.getvalue s#))))

(defmacro with-in-str
  "Evaluates body in a context in which *in* is bound to a fresh StringIO
  initialized with s."
  [s & body]
  `(let [si# (clojure.lang.RT/string-io ~s)]
     (binding [*in* si#]
       ~@body)))

(defn intern
  "Finds or creates a var named by the symbol name in the namespace ns
  (which can be a symbol or a namespace), setting its root binding to
  val if supplied."
  ([ns name]
   (clojure.lang.RT/intern (the-ns ns) name))
  ([ns name val]
   (let [v (clojure.lang.RT/intern (the-ns ns) name)]
     (clojure.lang.RT/bind-root v val)
     v)))

(defn loaded-libs
  "Returns a set of symbols naming the currently loaded libs."
  [] @*loaded-libs*)

(defn requiring-resolve
  "Resolves namespace-qualified sym per resolve. If initial resolve fails,
  attempts to require sym's namespace and retries."
  [sym]
  (when (qualified-symbol? sym)
    (or (resolve sym)
        (do (require (symbol (namespace sym))) (resolve sym)))))

(defmacro refer-clojure
  "Same as (refer 'clojure.core <filters>)."
  [& filters]
  `(refer 'clojure.core ~@filters))

(defn random-sample
  "Returns items from coll with random probability of prob (0.0 to 1.0).
  Returns a transducer when no collection is provided."
  ([prob] (filter (fn [_] (< (rand) prob))))
  ([prob coll] (clojure.lang.RT/random-sample-impl prob coll)))

;; --- update-vals / update-keys (need transient/persistent!) -------------

(defn update-vals
  "Given a map m and a function f of 1-argument, returns a new map
  where the keys of m are mapped to result of applying f to the
  corresponding values of m."
  [m f]
  (with-meta
    (persistent!
      (reduce-kv (fn [acc k v] (assoc! acc k (f v)))
                 (transient {})
                 m))
    (meta m)))

(defn update-keys
  "Given a map m and a function f of 1-argument, returns a new map
  whose keys are the result of applying f to the keys of m."
  [m f]
  (let [ret (persistent!
              (reduce-kv (fn [acc k v] (assoc! acc (f k) v))
                         (transient {})
                         m))]
    (with-meta ret (meta m))))

;; --- Inst protocol (vanilla 6913) ---------------------------------------
;;
;; Inst exposes inst-ms* for time-instant types. Vanilla extends to
;; java.util.Date and java.time.Instant; we extend to Python's
;; datetime.datetime (and date by inheritance via the timestamp method).

(defprotocol Inst
  (inst-ms* [inst]))

;; Extend Inst to Python's datetime.datetime — fetched via an RT shim
;; because qualified Python class names (`datetime/datetime`) aren't a
;; thing in our resolver.
(extend-type (clojure.lang.RT/datetime-class)
  Inst
  (inst-ms* [inst] (clojure.lang.RT/inst-ms-impl inst)))

;; (Re-define inst-ms in terms of the protocol method, replacing the
;; earlier RT-shim wrapper, so user-extended Inst types work.)
(defn inst-ms
  "Return the number of milliseconds since the epoch for the given inst."
  [inst] (inst-ms* inst))

;; --- iteration (vanilla 7914) -------------------------------------------
;;
;; A seqable application of (step k). Simplified to a lazy-seq
;; implementation rather than vanilla's reify + Seqable + Reducible.

;; --- load / test / read+string / definline / add-classpath / compile ---

(defn load
  "Loads Clojure code from each path. Each path is interpreted relative
  to a clojure source root on sys.path. Names without leading '/' are
  resolved per the standard require resolution rules."
  [& paths]
  (doseq [p paths]
    (let [normalized (if (and (string? p) (.startswith p "/"))
                       (.lstrip p "/")
                       p)]
      (load-file (str normalized ".clj")))))

(defn test
  "test [v] finds fn at key :test in the metadata of v, calls it with no
  args, and returns :ok. If :test is missing, returns :no-test."
  [v]
  (let [f (:test (meta v))]
    (if f
      (do (f) :ok)
      :no-test)))

(defn read+string
  "Like read, returns [form whitespace-trimmed-source-string]. Source
  capture is stubbed to \"\" (our reader has no captureString analogue)."
  ([] (read+string *in*))
  ([stream]
   [(read stream) ""]))

(defmacro definline
  "Like defn, but the function never actually inlines (we don't have a
  JVM-bytecode inliner). Accepts the same shape as vanilla."
  [name & decl]
  (let [params (first (drop-while (complement vector?) decl))
        body   (rest (drop-while (complement vector?) decl))]
    `(defn ~name ~params ~@body)))

(defn add-classpath
  "DEPRECATED on JVM, stub on Python — sys.path manipulation belongs
  in user code, not via this fn."
  [_url] nil)

(defn compile
  "AOT compile for the JVM. No-op on Python — code is read+compiled
  per-form already."
  [_lib] nil)

;; --- clojure-version (vanilla 7217-7250) ---------------------------------

(def ^:dynamic *clojure-version*
  {:major 1 :minor 11 :incremental 0 :qualifier "port-py"})

(defn clojure-version
  "Returns clojure version as a printable string."
  []
  (str (:major *clojure-version*) "."
       (:minor *clojure-version*)
       (when-let [i (:incremental *clojure-version*)]
         (str "." i))
       (when-let [q (:qualifier *clojure-version*)]
         (when (pos? (count q)) (str "-" q)))))

;; --- seq-to-map-for-destructuring (vanilla 1.11+) -----------------------
;;
;; Used by the destructuring code path for keyword arguments. Builds a
;; map from a seq: a single-arg trailing map is used as-is; otherwise
;; the seq is treated as alternating k/v pairs.

(defn seq-to-map-for-destructuring
  "Builds a map from a seq as described in the keyword-arguments spec.
  Used internally by destructuring."
  [s]
  (if (next s)
    (apply array-map s)
    (if (seq s) (first s) {})))

;; --- with-loading-context — JVM ClassLoader binding, stub ---------------

(defmacro with-loading-context
  "JVM-only ClassLoader binding helper. Pure passthrough on Python —
  there's no class-loader to bind."
  [& body]
  `(do ~@body))

(defn iteration
  "Creates a lazy seq via repeated calls to step (a fn of token k).
  Options: :somef (default some?), :vf (default identity),
  :kf (default identity), :initk (default nil)."
  [step & opts]
  (let [m      (apply hash-map opts)
        somef  (get m :somef some?)
        vf     (get m :vf identity)
        kf     (get m :kf identity)
        initk  (get m :initk nil)
        walk   (fn walk [k]
                 (lazy-seq
                   (let [ret (step k)]
                     (when (somef ret)
                       (let [v (vf ret)
                             nk (kf ret)]
                         (cons v (when nk (walk nk))))))))]
    (walk initk)))

;; ============================================================================
;; Deferred blocks — see `doc/port_audit.md` for the full catalog.
;;
;; Deferred permanently:
;;   structs  (vanilla 4068-4101): create-struct, defstruct, struct-map,
;;                                 struct, accessor — legacy API, not worth
;;                                 porting.
;;
;; Partial:
;;   defrecord — MVP only (positional + map constructors, keyword field
;;               access, protocol impls). Structural equality / hashing
;;               / full IPersistentMap (assoc/dissoc returning a new
;;               record) are deferred.
;;
;; Future work past the protocol/record cluster: futures, require/use/load,
;; stdlib modules (clojure.string / clojure.set / clojure.test).
;; ============================================================================

