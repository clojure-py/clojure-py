(ns examples.ants
  "Rich Hickey's 2009 Ant colony simulation, ported to clojure-py and Tk.
  Strict port of <https://gist.github.com/michiakig/1093917>: same world
  dimensions, same ant count, same rules. Departures from the original:
  (a) Tk method calls instead of Swing, (b) `(:import [tkinter ...])` at
  the top, and (c) `defrecord` Cell/Ant in place of `defstruct` (clojure-py
  does not implement defstruct)."
  (:import [tkinter Tk Canvas]))

;; --- Dimensions and rates ---------------------------------------------
;; All values are verbatim from the 2009 source.

(def dim 80)
(def nants-sqrt 7)
(def food-places 35)
(def food-range 100)
(def pher-scale 20.0)
(def food-scale 30.0)
(def evap-rate 0.99)
(def animation-sleep-ms 100)
(def ant-sleep-ms 40)
(def evap-sleep-ms 1000)
(def running true)

;; --- Records ----------------------------------------------------------
;; Original uses `(defstruct cell :food :pher)` and `(defstruct ant :dir)`.
;; clojure-py has no defstruct; defrecord with all-possible-fields up
;; front is the substitute. `:ant` is nil on an empty cell; `:home` is
;; false on a non-home cell. `:food` on an ant is false until laden.

(defrecord Cell [food pher ant home])
(defrecord Ant  [dir food])

;; --- World ------------------------------------------------------------

(def world
  (apply vector
         (map (fn [_]
                (apply vector
                       (map (fn [_] (ref (->Cell 0 0 nil false))) (range dim))))
              (range dim))))

(defn place [[x y]]
  (-> world (nth x) (nth y)))

;; --- Ant constructor + home offsets -----------------------------------

(defn create-ant
  "Create an ant agent at `loc` and write a fresh Ant into the cell at
  that location. Returns the agent."
  [loc dir]
  (dosync
    (let [p (place loc)]
      (alter p assoc :ant (->Ant dir false))
      (agent loc))))

(def home-off (/ dim 4))
(def home-range (range home-off (+ nants-sqrt home-off)))

;; --- Geometry / RNG helpers -------------------------------------------

(defn bound
  "Wrap n into [0, b)."
  [b n]
  (let [n (rem n b)]
    (if (neg? n) (+ n b) n)))

(defn wrand
  "Given a vector of slice sizes, return the index of a slice picked at
  random in proportion to its size."
  [slices]
  (let [total (reduce + slices)
        r (rand total)]
    (loop [i 0 sum 0]
      (if (< r (+ (slices i) sum))
        i
        (recur (inc i) (+ (slices i) sum))))))

(def dir-delta
  {0 [0 -1] 1 [1 -1] 2 [1 0] 3 [1 1]
   4 [0 1]  5 [-1 1] 6 [-1 0] 7 [-1 -1]})

(defn delta-loc
  "Move from [x y] one cell in direction `dir` (0..7), wrapping around
  the world."
  [[x y] dir]
  (let [[dx dy] (dir-delta (bound 8 dir))]
    [(bound dim (+ x dx)) (bound dim (+ y dy))]))
