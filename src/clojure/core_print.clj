;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(in-ns 'clojure.core)

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;; printing ;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;;
;; Adaptations from JVM core_print.clj:
;;   - No `(import '(java.io Writer))` — Python file-likes / StringIO have
;;     `.write` natively; the compiler's _fallback_append handles `.append`.
;;   - Float / Double / Long all collapse to Python int/float; we register
;;     print-method on Number once and special-case Inf/NaN for floats.
;;   - py.re/Pattern → Python re.Pattern (alias not needed; resolves
;;     via the dotted-name fallback as `py.re/Pattern`).
;;   - Skipped: print-method/print-dup for IRecord, TaggedLiteral,
;;     ReaderConditional, Throwable / StackTraceElement, PrintWriter-on. These
;;     depend on infra (records / proxy / Java stack traces) we haven't ported
;;     yet. A minimal Throwable print-method falls back to (str e).
;;   - Skipped: prefer-method calls that reference java.util.Map /
;;     Collection / List / RandomAccess — Python's list/dict/set are leaf
;;     collections that we render via separate print-method's on Object.

;;; --- dynamic vars --------------------------------------------------------

(def ^:dynamic
 ^{:doc "*print-length* controls how many items of each collection the
  printer will print. If it is bound to logical false, there is no
  limit. Otherwise, it must be bound to an integer indicating the maximum
  number of items of each collection to print. If a collection contains
  more items, the printer will print items up to the limit followed by
  '...' to represent the remaining items. The root binding is nil
  indicating no limit."
   :added "1.0"}
 *print-length* nil)

(def ^:dynamic
 ^{:doc "*print-level* controls how many levels deep the printer will
  print nested objects. If it is bound to logical false, there is no
  limit. Otherwise, it must be bound to an integer indicating the maximum
  level to print. Each argument to print is at level 0; if an argument is a
  collection, its items are at level 1; and so on. If an object is a
  collection and is at a level greater than or equal to the value bound to
  *print-level*, the printer prints '#' to represent it. The root binding
  is nil indicating no limit."
   :added "1.0"}
 *print-level* nil)

(def ^:dynamic *verbose-defrecords* false)

(def ^:dynamic
 ^{:doc "*print-namespace-maps* controls whether the printer will print
  namespace map literal syntax. It defaults to false, but the REPL binds
  to true."
   :added "1.9"}
 *print-namespace-maps* false)

;;; --- helpers -------------------------------------------------------------

(defn- print-sequential [^String begin, print-one, ^String sep, ^String end, sequence, w]
  (binding [*print-level* (and (not *print-dup*) *print-level* (dec *print-level*))]
    (if (and *print-level* (neg? *print-level*))
      (.write w "#")
      (do
        (.write w begin)
        (when-let [xs (seq sequence)]
          (if (and (not *print-dup*) *print-length*)
            (loop [[x & xs] xs
                   print-length *print-length*]
              (if (zero? print-length)
                (.write w "...")
                (do
                  (print-one x w)
                  (when xs
                    (.write w sep)
                    (recur xs (dec print-length))))))
            (loop [[x & xs] xs]
              (print-one x w)
              (when xs
                (.write w sep)
                (recur xs)))))
        (.write w end)))))

(defn- print-meta [o, w]
  (when-let [m (meta o)]
    (when (and (pos? (count m))
               (or *print-dup*
                   (and *print-meta* *print-readably*)))
      (.write w "^")
      (if (and (= (count m) 1) (:tag m))
          (pr-on (:tag m) w)
          (pr-on m w))
      (.write w " "))))

(defn print-simple [o, w]
  (print-meta o w)
  (.write w (str o)))

(defmethod print-method :default [o, w]
  (print-simple o w))

(defmethod print-method nil [o, w]
  (.write w "nil"))

(defmethod print-dup nil [o w] (print-method o w))

(defn print-ctor [o print-args w]
  (.write w "#=(")
  (.write w (.getName (class o)))
  (.write w ". ")
  (print-args o w)
  (.write w ")"))

(defn- print-tagged-object [o rep w]
  (when (instance? clojure.lang.IMeta o)
    (print-meta o w))
  (.write w "#object[")
  (let [c (class o)]
    (if (.isArray c)
      (print-method (.getName c) w)
      (.write w (.getName c))))
  (.write w " ")
  (.write w (format "0x%x " (System/identityHashCode o)))
  (print-method rep w)
  (.write w "]"))

(defn- print-object [o, w]
  (print-tagged-object o (str o) w))

(defmethod print-method Object [o, w]
  (print-object o w))

(defmethod print-method clojure.lang.Keyword [o, w]
  (.write w (str o)))

(defmethod print-dup clojure.lang.Keyword [o w] (print-method o w))

(defmethod print-method Number [o, w]
  ;; Python collapses Long / Double / Float to int / float. Handle Inf/NaN
  ;; for floats; let str() do the rest.
  (let [s (str o)]
    (cond
      (= s "inf")    (.write w "##Inf")
      (= s "-inf")   (.write w "##-Inf")
      (= s "nan")    (.write w "##NaN")
      :else          (.write w s))))

(defmethod print-dup Number [o, w]
  (print-ctor o
              (fn [o w]
                  (print-dup (str o) w))
              w))

;; prefer-method calls that depend on Fn / IPersistentCollection /
;; java.util.Collection. We have IPersistentCollection but not Fn as a marker;
;; use IFn (which Python fns are registered with).
(prefer-method print-dup clojure.lang.IPersistentCollection clojure.lang.IFn)

(defmethod print-method Boolean [o, w]
  (.write w (if o "true" "false")))

(defmethod print-dup Boolean [o w] (print-method o w))

(defmethod print-method clojure.lang.Symbol [o, w]
  (print-simple o w))

(defmethod print-dup clojure.lang.Symbol [o w] (print-method o w))

(defmethod print-method clojure.lang.Var [o, w]
  (print-simple o w))

(defmethod print-dup clojure.lang.Var [o, w]
  (.write w (str "#=(var " (.name (.ns o)) "/" (.sym o) ")")))

(defmethod print-method clojure.lang.ISeq [o, w]
  (print-meta o w)
  (print-sequential "(" pr-on " " ")" o w))

(defmethod print-dup clojure.lang.ISeq [o w] (print-method o w))
(defmethod print-dup clojure.lang.IPersistentList [o w] (print-method o w))
(prefer-method print-method clojure.lang.ISeq clojure.lang.IPersistentCollection)
(prefer-method print-dup clojure.lang.ISeq clojure.lang.IPersistentCollection)

(defmethod print-dup clojure.lang.IPersistentCollection [o, w]
  (print-meta o w)
  (.write w "#=(")
  (.write w (.getName (class o)))
  (.write w "/create ")
  (print-sequential "[" print-dup " " "]" o w)
  (.write w ")"))

(def ^{:tag String
       :doc "Returns escape string for char or nil if none"
       :added "1.0"}
  char-escape-string
    {\newline "\\n"
     \tab  "\\t"
     \return "\\r"
     \" "\\\""
     \\  "\\\\"
     \formfeed "\\f"
     \backspace "\\b"})

(defmethod print-method String [^String s, w]
  (if (or *print-dup* *print-readably*)
    (do (.append w \")
      (dotimes [n (count s)]
        (let [c (.charAt s n)
              e (char-escape-string c)]
          (if e (.write w e) (.append w c))))
      (.append w \"))
    (.write w s))
  nil)

(defmethod print-dup String [s w] (print-method s w))

(defmethod print-method clojure.lang.IPersistentVector [v, w]
  (print-meta v w)
  (print-sequential "[" pr-on " " "]" v w))

(defn- print-prefix-map [prefix kvs print-one w]
  (print-sequential
    (str prefix "{")
    (fn [[k v] w]
      (do (print-one k w) (.append w \space) (print-one v w)))
    ", "
    "}"
    kvs w))

(defn- print-map [m print-one w]
  (print-prefix-map nil m print-one w))

(defn- strip-ns
  [named]
  (if (symbol? named)
    (symbol nil (name named))
    (keyword nil (name named))))

(defn- lift-ns
  "Returns [lifted-ns lifted-kvs] or nil if m can't be lifted."
  [m]
  (when *print-namespace-maps*
    (loop [ns nil
           [[k v :as entry] & entries] (seq m)
           kvs []]
      (if entry
        (when (qualified-ident? k)
          (if ns
            (when (= ns (namespace k))
              (recur ns entries (conj kvs [(strip-ns k) v])))
            (when-let [new-ns (namespace k)]
              (recur new-ns entries (conj kvs [(strip-ns k) v])))))
        [ns kvs]))))

(defmethod print-method clojure.lang.IPersistentMap [m, w]
  (print-meta m w)
  (let [[ns lift-kvs] (lift-ns m)]
    (if ns
      (print-prefix-map (str "#:" ns) lift-kvs pr-on w)
      (print-map m pr-on w))))

(defmethod print-dup clojure.lang.IPersistentMap [m, w]
  (print-meta m w)
  (.write w "#=(")
  (.write w (.getName (class m)))
  (.write w "/create ")
  (print-map m print-dup w)
  (.write w ")"))

(defmethod print-method clojure.lang.IPersistentSet [s, w]
  (print-meta s w)
  (print-sequential "#{" pr-on " " "}" (seq s) w))

(def ^{:tag String
       :doc "Returns name string for char or nil if none"
       :added "1.0"}
 char-name-string
   {\newline "newline"
    \tab "tab"
    \space "space"
    \backspace "backspace"
    \formfeed "formfeed"
    \return "return"})

;; Note: Python has no Character type distinct from String — single-char
;; strings dispatch on String. The reader emits `\x` syntax as one-char
;; strings, so reader-printed chars look like quoted strings here. That's a
;; deliberate simplification of the JVM model.

(defmethod print-dup clojure.lang.Ratio [o w] (print-method o w))
(defmethod print-dup clojure.lang.BigInt [o w] (print-method o w))

(defmethod print-method Class [c, w]
  (.write w (.getName c)))

(defmethod print-dup Class [c, w]
  (.write w "#=")
  (.write w (.getName c)))

(defmethod print-method clojure.lang.BigDecimal [b, w]
  (.write w (str b))
  (.write w "M"))

(defmethod print-dup clojure.lang.BigDecimal [o w] (print-method o w))

(defmethod print-method clojure.lang.BigInt [b, w]
  (.write w (str b))
  (.write w "N"))

(defmethod print-method py.re/Pattern [p w]
  ;; JVM Pattern.pattern() is a method; Python re.Pattern.pattern is a string
  ;; attribute, so use field access (`.-`) here.
  (.write w "#\"")
  (.write w (.-pattern p))
  (.write w "\""))

(defmethod print-dup py.re/Pattern [p w] (print-method p w))

(defmethod print-method clojure.lang.Namespace [n w]
  (.write w "#namespace[")
  (.write w (str (ns-name n)))
  (.write w "]"))

(defmethod print-dup clojure.lang.Namespace [n w]
  (.write w "#=(find-ns ")
  (print-dup (ns-name n) w)
  (.write w ")"))

;; Minimal Throwable handling — JVM source builds a structured map from the
;; stack trace. We just print the type name and message; deeper introspection
;; can come once we have proper stack-trace machinery.
(defmethod print-method Throwable [t w]
  (.write w "#error \"")
  (.write w (.getName (class t)))
  (.write w ": ")
  (.write w (or (ex-message t) ""))
  (.write w "\""))

;;; --- IDeref printing -----------------------------------------------------

(defn- deref-as-map [o]
  (let [pending (and (instance? clojure.lang.IPending o)
                     (not (.is_realized o)))
        [ex val]
        (when-not pending
          (try [false (deref o)]
               (catch Throwable e
                 [true e])))]
    {:status
     (cond
      (or ex
          (and (instance? clojure.lang.Agent o)
               (agent-error o)))
      :failed

      pending
      :pending

      :else
      :ready)

     :val val}))

(defmethod print-method clojure.lang.IDeref [o w]
  (print-tagged-object o (deref-as-map o) w))

(def ^{:private true} print-initialized true)
