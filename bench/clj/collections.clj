;; Persistent-collection microbenchmarks.
;;
;; Key-variation benchmarks exist for maps because hashing + equality cost
;; is very different for keywords (identity), strings (bytewise), and ints.

(ns bench.collections)

;; ---------- Vector ----------

(defn vector-conj
  "Build a vector of size n by repeated conj. Stresses the PVector trie tail."
  [n]
  (loop [i 0 v []]
    (if (< i n) (recur (inc i) (conj v i)) v)))

(defn vector-conj-transient
  "Same size via transient — should be substantially faster."
  [n]
  (loop [i 0 v (transient [])]
    (if (< i n)
      (recur (inc i) (conj! v i))
      (persistent! v))))

(defn vector-nth [v n]
  (loop [i 0 s 0]
    (if (< i n) (recur (inc i) (+ s (nth v i))) s)))

(defn vector-assoc
  "Walk the vector replacing each element; tests assoc on a size-n PVector."
  [v n]
  (loop [i 0 w v]
    (if (< i n) (recur (inc i) (assoc w i (- i))) w)))

;; ---------- Hash-map / array-map ----------

(defn build-map-keywords [n]
  (persistent!
    (loop [i 0 m (transient {})]
      (if (< i n)
        (recur (inc i) (assoc! m (keyword (str "k" i)) i))
        m))))

(defn build-map-strings [n]
  (persistent!
    (loop [i 0 m (transient {})]
      (if (< i n)
        (recur (inc i) (assoc! m (str "k" i) i))
        m))))

(defn build-map-ints [n]
  (persistent!
    (loop [i 0 m (transient {})]
      (if (< i n)
        (recur (inc i) (assoc! m i i))
        m))))

(defn map-assoc-persistent
  "Persistent assoc n keys (no transient)."
  [ks]
  (loop [ks ks m {}]
    (if (seq ks)
      (recur (rest ks) (assoc m (first ks) (first ks)))
      m)))

(defn map-get-hits [m ks]
  (loop [ks ks s 0]
    (if (seq ks)
      (recur (rest ks) (+ s (get m (first ks) 0)))
      s)))

(defn map-dissoc-all [m ks]
  (loop [ks ks m m]
    (if (seq ks)
      (recur (rest ks) (dissoc m (first ks)))
      m)))

;; ---------- Set ----------

(defn set-conj [n]
  (loop [i 0 s #{}]
    (if (< i n) (recur (inc i) (conj s i)) s)))

(defn set-contains [s n]
  (loop [i 0 c 0]
    (if (< i n)
      (recur (inc i) (if (contains? s i) (inc c) c))
      c)))

;; ---------- List / seq walk ----------

(defn list-walk [xs]
  (loop [xs (seq xs) s 0]
    (if xs (recur (next xs) (+ s (first xs))) s)))

;; ---------- Prebuilt fixtures ----------

(def ^:private V10k  (vector-conj-transient 10000))
(def ^:private KW10k (map keyword (map #(str "k" %) (range 10000))))
(def ^:private KW8   (take 8 KW10k))     ; stays array-map
(def ^:private KW100 (take 100 KW10k))   ; promoted to HAMT
(def ^:private M10k-kw  (build-map-keywords 10000))
(def ^:private M10k-str (build-map-strings 10000))
(def ^:private M10k-int (build-map-ints 10000))
(def ^:private S10k     (set-conj 10000))
(def ^:private L10k     (apply list (range 10000)))

(def benchmarks
  {;; vector
   "coll/vector-conj-10k"             (fn [] (vector-conj 10000))
   "coll/vector-conj-transient-10k"   (fn [] (vector-conj-transient 10000))
   "coll/vector-nth-10k"              (fn [] (vector-nth V10k 10000))
   "coll/vector-assoc-10k"            (fn [] (vector-assoc V10k 10000))

   ;; map — small (array-map) vs large (HAMT)
   "coll/map-assoc-persistent-kw-8"   (fn [] (map-assoc-persistent KW8))
   "coll/map-assoc-persistent-kw-100" (fn [] (map-assoc-persistent KW100))
   "coll/map-build-kw-10k-transient"  (fn [] (build-map-keywords 10000))
   "coll/map-build-str-10k-transient" (fn [] (build-map-strings 10000))
   "coll/map-build-int-10k-transient" (fn [] (build-map-ints 10000))
   "coll/map-get-kw-10k"              (fn [] (map-get-hits M10k-kw KW10k))
   "coll/map-get-str-10k"             (fn [] (map-get-hits M10k-str (map #(str "k" %) (range 10000))))
   "coll/map-get-int-10k"             (fn [] (map-get-hits M10k-int (range 10000)))
   "coll/map-dissoc-all-10k-kw"       (fn [] (map-dissoc-all M10k-kw KW10k))

   ;; set
   "coll/set-conj-10k"                (fn [] (set-conj 10000))
   "coll/set-contains-10k"            (fn [] (set-contains S10k 10000))

   ;; seq walk
   "coll/list-walk-10k"               (fn [] (list-walk L10k))})
