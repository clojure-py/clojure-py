;; Word-count macrobench — regex tokenize + hash-map accumulation.
;; Builds a 200-line blob of "lorem-ipsum"-style text, then counts words.
;; Exercises: lazy re-seq walk, string hashing as map keys, incremental
;; map growth, and map iteration at the end.

(ns bench.wordcount
  (:require [clojure.string :as s]))

(def ^:private words
  ["lorem" "ipsum" "dolor" "sit" "amet" "consectetur" "adipiscing" "elit"
   "sed" "do" "eiusmod" "tempor" "incididunt" "ut" "labore" "et" "dolore"
   "magna" "aliqua" "enim" "ad" "minim" "veniam" "quis" "nostrud"
   "exercitation" "ullamco" "laboris" "nisi" "aliquip" "ex" "ea" "commodo"
   "consequat" "duis" "aute" "irure" "in" "reprehenderit" "voluptate"
   "velit" "esse" "cillum" "fugiat" "nulla" "pariatur" "excepteur" "sint"
   "occaecat" "cupidatat" "non" "proident" "sunt" "culpa" "qui" "officia"
   "deserunt" "mollit" "anim" "id" "est" "laborum"])

(defn gen-text [seed size]
  ;; Deterministic pseudo-random word picker — cheap LCG. We don't need
  ;; 32-bit wrap here; letting the int grow is fine because `mod` with
  ;; `n` (< 100) stays within reach.
  (let [n (count words)]
    (loop [i 0 x seed sb (transient [])]
      (if (< i size)
        (let [x' (+ (* x 1103515245) 12345)]
          (recur (inc i) x'
                 (conj! sb (nth words (mod (if (neg? x') (- x') x') n)))))
        (s/join " " (persistent! sb))))))

(def ^:private TEXT5k  (gen-text 42 5000))
(def ^:private TEXT20k (gen-text 42 20000))

(defn word-count-reduce
  "Reduce over re-seq hits, building a frequency map."
  [text]
  (reduce (fn [m w] (assoc m w (inc (get m w 0))))
          {}
          (re-seq #"\w+" text)))

(defn word-count-frequencies
  "Same, via core `frequencies`."
  [text]
  (frequencies (re-seq #"\w+" text)))

(defn word-count-transient
  "Manual transient accumulation — hand-rolled, should be fastest."
  [text]
  (persistent!
    (reduce (fn [m w] (assoc! m w (inc (get m w 0))))
            (transient {})
            (re-seq #"\w+" text))))

(def benchmarks
  {"wc/reduce-5k"         (fn [] (word-count-reduce TEXT5k))
   "wc/reduce-20k"        (fn [] (word-count-reduce TEXT20k))
   "wc/frequencies-5k"    (fn [] (word-count-frequencies TEXT5k))
   "wc/frequencies-20k"   (fn [] (word-count-frequencies TEXT20k))
   "wc/transient-20k"     (fn [] (word-count-transient TEXT20k))})
