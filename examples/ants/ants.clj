(ns examples.ants
  "Rich Hickey's 2009 Ant colony simulation, ported to clojure-py and Tk.
  Strict port of <https://gist.github.com/michiakig/1093917>: same world
  dimensions, same ant count, same rules. Departures from the original:
  (a) Tk method calls instead of Swing, (b) `(:import [tkinter ...])` at
  the top, and (c) `defrecord` Cell/Ant in place of `defstruct` (clojure-py
  does not implement defstruct)."
  (:import [tkinter Tk Canvas]
           [time sleep]
           [queue Queue]
           [builtins dict]
           os))

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

;; --- Ant behaviour --------------------------------------------------------

(defn turn
  "turns the ant at the location by the given amount"
  [loc amt]
  (dosync
   (let [p (place loc)
         ant (:ant @p)]
     (alter p assoc :ant (assoc ant :dir (bound 8 (+ (:dir ant) amt))))))
  loc)

(defn move
  "moves the ant in the direction it is heading. Must be called in a
  transaction that has verified the way is clear"
  [loc]
  (let [oldp (place loc)
        ant (:ant @oldp)
        newloc (delta-loc loc (:dir ant))
        p (place newloc)]
    ;move the ant
    (alter p assoc :ant ant)
    (alter oldp assoc :ant nil)
    ;leave pheromone trail
    (when-not (:home @oldp)
      (alter oldp assoc :pher (inc (:pher @oldp))))
    newloc))

(defn take-food [loc]
  "Takes one food from current location. Must be called in a
  transaction that has verified there is food available"
  (let [p (place loc)
        ant (:ant @p)]
    (alter p assoc
           :food (dec (:food @p))
           :ant (assoc ant :food true))
    loc))

(defn drop-food [loc]
  "Drops food at current location. Must be called in a
  transaction that has verified the ant has food"
  (let [p (place loc)
        ant (:ant @p)]
    (alter p assoc
           :food (inc (:food @p))
           :ant (assoc ant :food nil))
    loc))

(defn rank-by
  "returns a map of xs to their 1-based rank when sorted by keyfn"
  [keyfn xs]
  (let [sorted (sort-by (comp float keyfn) xs)]
    (reduce (fn [ret i] (assoc ret (nth sorted i) (inc i)))
            {} (range (count sorted)))))

(defn behave
  "the main function for the ant agent"
  [loc]
  (let [p (place loc)
        ant (:ant @p)
        ahead (place (delta-loc loc (:dir ant)))
        ahead-left (place (delta-loc loc (dec (:dir ant))))
        ahead-right (place (delta-loc loc (inc (:dir ant))))
        places [ahead ahead-left ahead-right]]
    (sleep (/ ant-sleep-ms 1000.0))
    (dosync
     (when running
       (send-off *agent* #'behave))
     (if (:food ant)
       ;going home
       (cond
        (:home @p)
          (-> loc drop-food (turn 4))
        (and (:home @ahead) (not (:ant @ahead)))
          (move loc)
        :else
          (let [ranks (merge-with +
                        (rank-by (comp #(if (:home %) 1 0) deref) places)
                        (rank-by (comp :pher deref) places))]
          (([move #(turn % -1) #(turn % 1)]
            (wrand [(if (:ant @ahead) 0 (ranks ahead))
                    (ranks ahead-left) (ranks ahead-right)]))
           loc)))
       ;foraging
       (cond
        (and (pos? (:food @p)) (not (:home @p)))
          (-> loc take-food (turn 4))
        (and (pos? (:food @ahead)) (not (:home @ahead)) (not (:ant @ahead)))
          (move loc)
        :else
          (let [ranks (merge-with +
                                  (rank-by (comp :food deref) places)
                                  (rank-by (comp :pher deref) places))]
          (([move #(turn % -1) #(turn % 1)]
            (wrand [(if (:ant @ahead) 0 (ranks ahead))
                    (ranks ahead-left) (ranks ahead-right)]))
           loc)))))))

;; --- Evaporation ----------------------------------------------------------

(defn evaporate
  "causes all the pheromones to evaporate a bit"
  []
  (dorun
   (for [x (range dim) y (range dim)]
     (dosync
      (let [p (place [x y])]
        (alter p assoc :pher (* evap-rate (:pher @p))))))))

(defn evaporation
  "agent action: evaporate once, sleep, then re-send itself"
  [_]
  (when running
    (send-off *agent* #'evaporation))
  (evaporate)
  (sleep (/ evap-sleep-ms 1000.0))
  nil)

;; --- World setup ----------------------------------------------------------

(defn setup
  "places initial food and ants, returns seq of ant agents"
  []
  (dosync
    (dotimes [i food-places]
      (let [p (place [(rand-int dim) (rand-int dim)])]
        (alter p assoc :food (rand-int food-range))))
    (doall
     (for [x home-range y home-range]
       (do
         (alter (place [x y])
                assoc :home true)
         (create-ant [x y] (rand-int 8)))))))

;; --- Tk bridge ----------------------------------------------------------

(def scale 8)           ;; pixels per world cell — bumped from original's 5 for Tk
(def render-queue (Queue 8))   ;; bounded; drop on overflow (we're behind, not wrong)

(defn snapshot-world
  "Inside one dosync, build a flat seq of cell records suitable for the
  renderer. Frame is internally consistent."
  []
  (dosync
    (vec
      (for [x (range dim) y (range dim)
            :let [c @(place [x y])]]
        {:x x :y y
         :pher (:pher c) :food (:food c)
         :home (:home c) :ant (:ant c)}))))

;; --- Tk colour helpers --------------------------------------------------

(defn pher-color
  "Tk color string for a pheromone level."
  [pher]
  (let [g (max 0 (min 255 (int (* 255 (/ pher pher-scale)))))]
    (format "#00%02x00" g)))

(defn food-color
  "Tk color string for a food level."
  [food]
  (let [r (max 0 (min 255 (int (* 255 (/ food food-scale)))))]
    (format "#%02x0000" r)))

;; --- Tk rendering -------------------------------------------------------
;; kwargs do NOT work through (.method obj ...) — the VM uses call_method1
;; which is positional-only.  Tk canvas create_* methods accept an optional
;; `cnf` dict as their last positional argument, so we use
;;   (dict [["fill" "red"] ["outline" ""]])
;; instead of :fill "red" :outline "".

(defn ant-triangle
  "Return [px1 py1 px2 py2 px3 py3] — the 6 polygon coords for an ant
  triangle inscribed in cell (x0,y0)..(+ x0 scale, + y0 scale), pointing
  in direction `dir` (0..7). Adapted from the original's render-ant."
  [x0 y0 dir]
  (let [cx (+ x0 (/ scale 2.0))
        cy (+ y0 (/ scale 2.0))
        [dx dy] (dir-delta dir)
        ;; Diagonal directions have unit length sqrt(2); shrink so visual
        ;; ant size stays roughly constant across all 8 directions.
        norm (if (and (not= dx 0) (not= dy 0)) 0.7071 1.0)
        fx (* dx norm (/ scale 2.0))
        fy (* dy norm (/ scale 2.0))
        ;; Base center sits slightly behind the geometric center.
        bx (- cx (* dx norm (/ scale 3.0)))
        by (- cy (* dy norm (/ scale 3.0)))
        ;; Perpendicular to (dx,dy) is (-dy, dx); base half-width = scale/3.
        px (* (- dy) norm (/ scale 3.0))
        py (* dx norm (/ scale 3.0))]
    [(+ cx fx) (+ cy fy)
     (+ bx px) (+ by py)
     (- bx px) (- by py)]))

(defn render-cell [canvas cell]
  (let [{:keys [x y pher food home ant]} cell
        x0 (* x scale) y0 (* y scale)
        x1 (+ x0 scale) y1 (+ y0 scale)]
    (when (pos? pher)
      (.create_rectangle canvas x0 y0 x1 y1
                         (dict [["fill" (pher-color pher)] ["outline" ""]])))
    (when (pos? food)
      (.create_rectangle canvas (+ x0 1) (+ y0 1) (- x1 1) (- y1 1)
                         (dict [["fill" (food-color food)] ["outline" ""]])))
    (when home
      (.create_rectangle canvas x0 y0 x1 y1
                         (dict [["fill" ""] ["outline" "blue"]])))
    (when ant
      (let [color (if (:food ant) "red" "black")
            [tx1 ty1 tx2 ty2 tx3 ty3] (ant-triangle x0 y0 (:dir ant))]
        (.create_polygon canvas tx1 ty1 tx2 ty2 tx3 ty3
                         (dict [["fill" color] ["outline" ""]]))))))

(defn render
  "Drain the canvas, redraw the world snapshot, and frame it."
  [canvas]
  (.delete canvas "all")
  (doseq [c (snapshot-world)]
    (render-cell canvas c))
  (.create_rectangle canvas 0 0 (* dim scale) (* dim scale)
                     (dict [["fill" ""] ["outline" "black"]])))

;; --- Animator agent action ----------------------------------------------

(defn animation
  "Periodically push a render request to the queue."
  [_]
  (when running
    (send-off *agent* #'animation))
  (try (.put_nowait render-queue 1)
       (catch Exception _ nil))   ;; queue full — drop frame
  (sleep (/ animation-sleep-ms 1000.0))
  nil)

;; --- Launch -------------------------------------------------------------

(defn -drain-and-render
  "Tk-thread polling callback. If the queue has a render request, drain
  it (collapse multiple → one repaint) and redraw. Always re-schedules
  itself."
  [root canvas]
  (when-not (.empty render-queue)
    ;; Drain everything, then render once.
    (loop []
      (when-not (.empty render-queue)
        (try (.get_nowait render-queue) (catch Exception _ nil))
        (recur)))
    (render canvas))
  (.after root animation-sleep-ms
          (fn [] (-drain-and-render root canvas))))

(defn launch
  "Build the world, spawn agents, open a Tk window, and enter the event
  loop. Blocks until the window is closed."
  []
  (let [ants (setup)
        evap-agent (agent nil)
        anim-agent (agent nil)
        root (Tk)
        canvas (Canvas root
                       (dict [["width" (* dim scale)]
                              ["height" (* dim scale)]
                              ["bg" "white"]]))]
    (.title root "Ants")
    (.pack canvas)
    ;; Kick off ant agents
    (doseq [a ants] (send-off a #'behave))
    ;; Kick off evaporator + animator
    (send-off evap-agent #'evaporation)
    (send-off anim-agent #'animation)
    ;; Tk poll loop
    (.after root animation-sleep-ms
            (fn [] (-drain-and-render root canvas)))
    (.mainloop root)))

;; Auto-launch when run via `python -m clojure examples/ants/ants.clj`,
;; unless ANTS_NO_GUI=1 (used by the smoke test). clojure-py has no
;; System/getenv, so go through Python's os.environ directly.
(when (not (= "1" (.get (.-environ os) "ANTS_NO_GUI" "")))
  (launch))
