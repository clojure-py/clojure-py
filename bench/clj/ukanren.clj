;; μKanren — the 20-line core Clojure port used as a macrobenchmark.
;; Based on Byrd & Friedman's 2013 paper. Classic lazy-seq stream impl:
;; - goals are fns taking a substitution+counter and returning a stream
;; - streams are seqs (possibly containing thunks for inverse-η delay)
;; - mplus interleaves, bind maps-and-interleaves
;;
;; This is the canonical shape that Clojure-level minikanren ports use,
;; so it's the right workload to stress lazy-seq realization, closure
;; creation, and the recursive map-walking unify.

(ns bench.ukanren)

;; ---------- Logic vars ----------
;; Represent as tiny records so `=` is by identity-of-fields rather
;; than whatever accidental equality an ad-hoc {:lvar k} map gives us.
(defrecord LVar [n])
(defn lvar? [x] (instance? LVar x))
(defn lvar [n] (->LVar n))

;; ---------- Substitution ----------

(defn walk [u s]
  (if (lvar? u)
    (if-let [v (get s u)]
      (recur v s)
      u)
    u))

(defn ext-s [x v s] (assoc s x v))

(defn unify [u v s]
  (let [u (walk u s) v (walk v s)]
    (cond
      (and (lvar? u) (lvar? v) (= u v)) s
      (lvar? u) (ext-s u v s)
      (lvar? v) (ext-s v u s)
      (and (sequential? u) (sequential? v) (seq u) (seq v))
      (when-let [s (unify (first u) (first v) s)]
        (unify (rest u) (rest v) s))
      (and (not (sequential? u)) (not (sequential? v)))
      (when (= u v) s)
      :else nil)))

;; ---------- State: [substitution, counter] ----------

(def empty-state [{} 0])

;; ---------- Goal constructors ----------

(defn == [u v]
  (fn [[s c]]
    (if-let [s' (unify u v s)]
      (list [s' c])
      nil)))

(defn call-fresh [f]
  (fn [[s c]]
    ((f (lvar c)) [s (inc c)])))

(declare mplus bind)

(defn disj-g [g1 g2]
  (fn [sc] (mplus (g1 sc) (g2 sc))))

(defn conj-g [g1 g2]
  (fn [sc] (bind (g1 sc) g2)))

(defn mplus [s1 s2]
  (cond
    (nil? s1) s2
    (fn? s1)  (fn [] (mplus s2 (s1)))
    :else     (cons (first s1) (mplus (next s1) s2))))

(defn bind [s g]
  (cond
    (nil? s)  nil
    (fn? s)   (fn [] (bind (s) g))
    :else     (mplus (g (first s)) (bind (next s) g))))

;; ---------- Run — pull n results out of the stream ----------

(defn pull [s]
  (if (fn? s) (recur (s)) s))

(defn take-n [n s]
  (let [s (pull s)]
    (cond
      (nil? s) nil
      (zero? n) nil
      :else (cons (first s) (take-n (dec n) (next s))))))

(defn reify-term [v s]
  (let [v (walk v s)]
    (cond
      (lvar? v) v
      (and (sequential? v) (seq v))
      (cons (reify-term (first v) s) (reify-term (rest v) s))
      :else v)))

(defn run* [n g]
  (let [states (take-n n (g empty-state))]
    (map (fn [[s _]] (reify-term (lvar 0) s)) states)))

;; ---------- Classic benches ----------

(defn appendo [l r out]
  (disj-g
    (conj-g (== l '()) (== r out))
    (call-fresh
      (fn [a]
        (call-fresh
          (fn [d]
            (call-fresh
              (fn [res]
                (conj-g (== l (cons a d))
                  (conj-g (== out (cons a res))
                          (appendo d r res)))))))))))

(defn appendo-splits
  "All (l r) splits of a size-k list: n solutions."
  [k n]
  (let [target (range k)]
    (doall
      (run* n
        (call-fresh
          (fn [l]
            (call-fresh
              (fn [r]
                (appendo l r target)))))))))

;; ---------- N-unify ----------
;; Pure unification stress — build up a substitution of N pairs.

(defn unify-chain [n]
  (loop [i 0 s {}]
    (if (< i n)
      (recur (inc i) (unify (lvar i) i s))
      s)))

(def benchmarks
  {"ukanren/appendo-splits-8-100"  (fn [] (appendo-splits 8 100))
   "ukanren/appendo-splits-16-100" (fn [] (appendo-splits 16 100))
   "ukanren/unify-chain-10k"       (fn [] (unify-chain 10000))})
