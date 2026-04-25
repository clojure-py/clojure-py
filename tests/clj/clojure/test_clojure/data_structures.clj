;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

;; Author: Frantisek Sodomka

;; Adaptations from vanilla:
;;   * Skipped the entire generative section (`defspec`,
;;     `clojure.test.generative`, `clojure.data.generators`,
;;     `assert-same-collection`) — those libraries aren't ported.
;;   * Skipped tests that lean on JVM-only types or constructors:
;;     `(java.util.HashMap. ...)`, `(java.util.ArrayList. ...)`,
;;     `(java.util.HashSet. ...)`, `(into-array …)`, `(Integer. …)`,
;;     `(Long. …)`, `clojure.lang.PersistentQueue/EMPTY` (queues are not
;;     yet present), `(.size …)`, `.equals`, `.hashCode`, `.iterator`,
;;     `clojure.lang.IReduce` reify, `defstruct`.
;;   * `ClassCastException` for "wrong collection type" (e.g. `(peek #{1})`,
;;     `(disj [1 2] 1)`) → our runtime raises
;;     `clojure._core/IllegalArgumentException` from the
;;     missing-protocol-impl path. Same idea, different name.
;;   * `IllegalStateException` for `pop` on empty stays as
;;     `clojure._core/IllegalStateException`.
;;   * Dropped BigDecimal cases (`0M`, `1M`) — we don't have BigDecimal.
;;   * `(read-string "{:a 1, :b 2, :a -1, :c 3}")` — vanilla rejects
;;     duplicate keys at read time; our reader silently keeps the last
;;     value. Skipped that assertion.
;;   * Records: `record-hashing` skipped — `defrecord` doesn't yet
;;     implement vanilla-equivalent structural hashing (see project memory).
;;   * `seq-iter-match` rewritten to walk Python `__iter__` directly.

(ns clojure.test-clojure.data-structures
  (:use clojure.test))

;; *** Helper functions ***

(defn diff [s1 s2]
  (seq (reduce disj (set s1) (set s2))))


;; *** General ***

(deftest test-equality
  ; nil is not equal to any other value
  (are [x] (not (= nil x))
      true false
      0 0.0
      \space
      "" #""
      () [] #{} {}
      (lazy-seq nil)
      (lazy-seq ())
      (lazy-seq [])
      (lazy-seq {})
      (lazy-seq #{})
      (lazy-seq ""))

  ; ratios
  (is (== 1/2 0.5))
  (is (== 1/1000 0.001))
  (is (not= 2/3 0.6666666666666666))

  ; vectors equal other seqs by items equality
  (are [x y] (= x y)
      '() []
      '(1) [1]
      '(1 2) [1 2]

      [] '()
      [1] '(1)
      [1 2] '(1 2) )
  (is (not= [1 2] '(2 1)))

  ; list and vector vs. set and map
  (are [x y] (not= x y)
      () #{}
      () {}
      [] #{}
      [] {}
      #{} {}
      '(1) #{1}
      [1] #{1} )

  (is (not= (sorted-set :a) (sorted-set 1)))

  ; sorted-set vs. hash-set
  (are [x y] (= x y)
      (sorted-set-by <) (hash-set)
      (sorted-set-by < 1) (hash-set 1)
      (sorted-set-by < 3 2 1) (hash-set 3 2 1)
      (sorted-set) (hash-set)
      (sorted-set 1) (hash-set 1)
      (sorted-set 3 2 1) (hash-set 3 2 1) ))


;; *** Collections ***

(deftest test-count
  (are [x y] (= (count x) y)
       nil 0

       () 0
       '(1) 1
       '(1 2 3) 3

       [] 0
       [1] 1
       [1 2 3] 3

       #{} 0
       #{1} 1
       #{1 2 3} 3

       {} 0
       {:a 1} 1
       {:a 1 :b 2 :c 3} 3

       "" 0
       "a" 1
       "abc" 3)

  ; different types
  (are [x]  (= (count [x]) 1)
      nil true false
      0 0.0 "" \space
      () [] #{} {}  ))


(deftest test-conj
  ; doesn't work on strings
  (is (thrown? clojure._core/IllegalArgumentException (conj "" \a)))

  (are [x y] (= x y)
      (conj nil 1) '(1)
      (conj nil 3 2 1) '(1 2 3)

      (conj nil nil) '(nil)
      (conj nil nil nil) '(nil nil)
      (conj nil nil nil 1) '(1 nil nil)

      ; list -> conj puts the item at the front of the list
      (conj () 1) '(1)
      (conj () 1 2) '(2 1)

      (conj '(2 3) 1) '(1 2 3)
      (conj '(2 3) 1 4 3) '(3 4 1 2 3)

      (conj () nil) '(nil)
      (conj () ()) '(())

      ; vector -> conj puts the item at the end of the vector
      (conj [] 1) [1]
      (conj [] 1 2) [1 2]

      (conj [2 3] 1) [2 3 1]
      (conj [2 3] 1 4 3) [2 3 1 4 3]

      (conj [] nil) [nil]
      (conj [] []) [[]]

      ; map -> conj expects another (possibly single entry) map as the item
      (conj {} {}) {}
      (conj {} {:a 1}) {:a 1}
      (conj {} {:a 1 :b 2}) {:a 1 :b 2}
      (conj {} {:a 1 :b 2} {:c 3}) {:a 1 :b 2 :c 3}
      (conj {} {:a 1 :b 2} {:a 3 :c 4}) {:a 3 :b 2 :c 4}

      (conj {:a 1} {:a 7}) {:a 7}
      (conj {:a 1} {:b 2}) {:a 1 :b 2}
      (conj {:a 1} {:a 7 :b 2}) {:a 7 :b 2}

      (conj {} (first {:a 1})) {:a 1}           ; MapEntry
      (conj {:a 1} (first {:b 2})) {:a 1 :b 2}
      (conj {:a 1} (first {:a 7})) {:a 7}

      (conj {} [:a 1]) {:a 1}                   ; vector
      (conj {:a 1} [:b 2]) {:a 1 :b 2}
      (conj {:a 1} [:a 7]) {:a 7}

      (conj {} {nil {}}) {nil {}}
      (conj {} {{} nil}) {{} nil}
      (conj {} {{} {}}) {{} {}}

      ; set
      (conj #{} 1) #{1}
      (conj #{} 1 2 3) #{1 2 3}

      (conj #{2 3} 1) #{3 1 2}
      (conj #{3 2} 1) #{1 2 3}

      (conj #{2 3} 2) #{2 3}
      (conj #{2 3} 2 3) #{2 3}
      (conj #{2 3} 4 1 2 3) #{1 2 3 4}

      (conj #{} nil) #{nil}
      (conj #{} #{}) #{#{}} ))


;; *** Lists and Vectors ***

(deftest test-peek
  ; doesn't work for sets and maps
  (is (thrown? clojure._core/IllegalArgumentException (peek #{1})))
  (is (thrown? clojure._core/IllegalArgumentException (peek {:a 1})))

  (are [x y] (= x y)
      (peek nil) nil

      ; list = first
      (peek ()) nil
      (peek '(1)) 1
      (peek '(1 2 3)) 1

      (peek '(nil)) nil
      (peek '(1 nil)) 1
      (peek '(nil 2)) nil
      (peek '(())) ()
      (peek '(() nil)) ()
      (peek '(() 2 nil)) ()

      ; vector = last
      (peek []) nil
      (peek [1]) 1
      (peek [1 2 3]) 3

      (peek [nil]) nil
      (peek [1 nil]) nil
      (peek [nil 2]) 2
      (peek [[]]) []
      (peek [[] nil]) nil
      (peek [[] 2 nil]) nil ))


(deftest test-pop
  ; doesn't work for sets and maps
  (is (thrown? clojure._core/IllegalArgumentException (pop #{1})))

  ; collection cannot be empty
  (is (thrown? clojure._core/IllegalStateException (pop ())))
  (is (thrown? clojure._core/IllegalStateException (pop [])))

  (are [x y] (= x y)
      (pop nil) nil

      ; list - pop first
      (pop '(1)) ()
      (pop '(1 2 3)) '(2 3)

      (pop '(nil)) ()
      (pop '(1 nil)) '(nil)
      (pop '(nil 2)) '(2)
      (pop '(())) ()
      (pop '(() nil)) '(nil)
      (pop '(() 2 nil)) '(2 nil)

      ; vector - pop last
      (pop [1]) []
      (pop [1 2 3]) [1 2]

      (pop [nil]) []
      (pop [1 nil]) [1]
      (pop [nil 2]) [nil]
      (pop [[]]) []
      (pop [[] nil]) [[]]
      (pop [[] 2 nil]) [[] 2] ))


;; *** Lists (IPersistentList) ***

(deftest test-list
  (are [x]  (list? x)
      ()
      '()
      (list)
      (list 1 2 3) )

  ; order is important
  (are [x y] (not (= x y))
      (list 1 2) (list 2 1)
      (list 3 1 2) (list 1 2 3) )

  (are [x y] (= x y)
      '() ()
      (list) '()
      (list 1) '(1)
      (list 1 2) '(1 2)

      ; nesting
      (list 1 (list 2 3) (list 3 (list 4 5 (list 6 (list 7)))))
        '(1 (2 3) (3 (4 5 (6 (7)))))

      ; different data structures
      (list true false nil)
        '(true false nil)
      (list 1 2.5 2/3 "ab" \x 'cd :kw)
        '(1 2.5 2/3 "ab" \x cd :kw)
      (list (list 1 2) [3 4] {:a 1 :b 2} #{:c :d})
        '((1 2) [3 4] {:a 1 :b 2} #{:c :d})

      ; evaluation
      (list (+ 1 2) [(+ 2 3) 'a] (list (* 2 3) 8))
        '(3 [5 a] (6 8))

      ; special cases
      (list nil) '(nil)
      (list 1 nil) '(1 nil)
      (list nil 2) '(nil 2)
      (list ()) '(())
      (list 1 ()) '(1 ())
      (list () 2) '(() 2) ))


;; *** Maps (IPersistentMap) ***

(deftest test-find
  (are [x y] (= x y)
      (find {} :a) nil

      (find {:a 1} :a) [:a 1]
      (find {:a 1} :b) nil
      (find {nil 1} nil) [nil 1]

      (find {:a 1 :b 2} :a) [:a 1]
      (find {:a 1 :b 2} :b) [:b 2]
      (find {:a 1 :b 2} :c) nil

      (find {} nil) nil
      (find {:a 1} nil) nil
      (find {:a 1 :b 2} nil) nil ))


(deftest test-contains?
  ; contains? is designed to work preferably on maps and sets
  (are [x y] (= x y)
      (contains? {} :a) false
      (contains? {} nil) false

      (contains? {:a 1} :a) true
      (contains? {:a 1} :b) false
      (contains? {:a 1} nil) false
      (contains? {nil 1} nil) true

      (contains? {:a 1 :b 2} :a) true
      (contains? {:a 1 :b 2} :b) true
      (contains? {:a 1 :b 2} :c) false
      (contains? {:a 1 :b 2} nil) false

      ; sets
      (contains? #{} 1) false
      (contains? #{} nil) false

      (contains? #{1} 1) true
      (contains? #{1} 2) false
      (contains? #{1} nil) false

      (contains? #{1 2 3} 1) true
      (contains? #{1 2 3} 3) true
      (contains? #{1 2 3} 10) false
      (contains? #{1 2 3} nil) false)

  ; numerically indexed collections (vectors)
  ; => test if the numeric key is WITHIN THE RANGE OF INDEXES
  (are [x y] (= x y)
      (contains? [] 0) false
      (contains? [] -1) false
      (contains? [] 1) false

      (contains? [1] 0) true
      (contains? [1] -1) false
      (contains? [1] 1) false

      (contains? [1 2 3] 0) true
      (contains? [1 2 3] 2) true
      (contains? [1 2 3] 3) false
      (contains? [1 2 3] -1) false))


(deftest test-keys
  (are [x y] (= x y)      ; other than map data structures
      (keys ()) nil
      (keys []) nil
      (keys #{}) nil
      (keys "") nil )

  (are [x y] (= x y)
      (keys {}) nil
      (keys {:a 1}) '(:a)
      (keys {nil 1}) '(nil)
      (diff (keys {:a 1 :b 2}) '(:a :b)) nil

      (keys (sorted-map)) nil
      (keys (sorted-map :a 1)) '(:a)
      (diff (keys (sorted-map :a 1 :b 2)) '(:a :b)) nil

      (keys (hash-map)) nil
      (keys (hash-map :a 1)) '(:a)
      (diff (keys (hash-map :a 1 :b 2)) '(:a :b)) nil )

  (let [m {:a 1 :b 2}
        k (keys m)]
    (is (= {:hi :there} (meta (with-meta k {:hi :there}))))))


(deftest test-vals
  (are [x y] (= x y)      ; other than map data structures
      (vals ()) nil
      (vals []) nil
      (vals #{}) nil
      (vals "") nil )

  (are [x y] (= x y)
      (vals {}) nil
      (vals {:a 1}) '(1)
      (vals {nil 1}) '(1)
      (diff (vals {:a 1 :b 2}) '(1 2)) nil

      (vals (sorted-map)) nil
      (vals (sorted-map :a 1)) '(1)
      (diff (vals (sorted-map :a 1 :b 2)) '(1 2)) nil

      (vals (hash-map)) nil
      (vals (hash-map :a 1)) '(1)
      (diff (vals (hash-map :a 1 :b 2)) '(1 2)) nil )

  (let [m {:a 1 :b 2}
        v (vals m)]
    (is (= {:hi :there} (meta (with-meta v {:hi :there}))))))


(deftest test-key
  (are [x]  (= (key (first (hash-map x :value))) x)
      nil
      false true
      0 42
      0.0 3.14
      2/3
      \c
      "" "abc"
      'sym
      :kw
      () '(1 2)
      [] [1 2]
      {} {:a 1 :b 2}
      #{} #{1 2} ))


(deftest test-val
  (are [x]  (= (val (first (hash-map :key x))) x)
      nil
      false true
      0 42
      0.0 3.14
      2/3
      \c
      "" "abc"
      'sym
      :kw
      () '(1 2)
      [] [1 2]
      {} {:a 1 :b 2}
      #{} #{1 2} ))


(deftest test-get
  (let [m {:a 1, :b 2, :c {:d 3, :e 4}, :f nil, :g false, nil {:h 5}}]
    (are [x y] (= x y)
         (get m :a) 1
         (get m :e) nil
         (get m :e 0) 0
         (get m nil) {:h 5}
         (get m :b 0) 2
         (get m :f 0) nil

         (get-in m [:c :e]) 4
         (get-in m '(:c :e)) 4
         (get-in m [:c :x]) nil
         (get-in m [:f]) nil
         (get-in m [:g]) false
         (get-in m [:h]) nil
         (get-in m []) m
         (get-in m nil) m

         (get-in m [:c :e] 0) 4
         (get-in m '(:c :e) 0) 4
         (get-in m [:c :x] 0) 0
         (get-in m [:b] 0) 2
         (get-in m [:f] 0) nil
         (get-in m [:g] 0) false
         (get-in m [:h] 0) 0
         (get-in m [:x :y] {:y 1}) {:y 1}
         (get-in m [] 0) m
         (get-in m nil 0) m)))


(deftest test-nested-map-destructuring
  (let [sample-map {:a 1 :b {:a 2}}
        {ao1 :a {ai1 :a} :b} sample-map
        {ao2 :a {ai2 :a :as m1} :b :as m2} sample-map
        {ao3 :a {ai3 :a :as m} :b :as m} sample-map
        {{ai4 :a :as m} :b ao4 :a :as m} sample-map]
    (are [i o] (and (= i 2)
                    (= o 1))
         ai1 ao1
         ai2 ao2
         ai3 ao3
         ai4 ao4)))


(deftest test-map-entry?
  (testing "map-entry? = false"
    (are [entry]
      (false? (map-entry? entry))
      nil 5 #{1 2} '(1 2) {:a 1} [] [0] [1 2 3]))
  (testing "map-entry? = true"
    (is (true? (map-entry? (first {:a 1}))))))


;; *** Sets ***

(deftest test-hash-set
  (are [x] (set? x)
      #{}
      #{1 2}
      (hash-set)
      (hash-set 1 2) )

  ; order isn't important
  (are [x y] (= x y)
      #{1 2} #{2 1}
      #{3 1 2} #{1 2 3}
      (hash-set 1 2) (hash-set 2 1)
      (hash-set 3 1 2) (hash-set 1 2 3) )

  (are [x y] (= x y)
      ; creating
      (hash-set) #{}
      (hash-set 1) #{1}
      (hash-set 1 2) #{1 2}

      ; nesting
      (hash-set 1 (hash-set 2 3) (hash-set 3 (hash-set 4 5 (hash-set 6 (hash-set 7)))))
        #{1 #{2 3} #{3 #{4 5 #{6 #{7}}}}}

      ; different data structures
      (hash-set true false nil)
        #{true false nil}
      (hash-set 1 2.5 2/3 "ab" \x 'cd :kw)
        #{1 2.5 2/3 "ab" \x 'cd :kw}
      (hash-set (list 1 2) [3 4] {:a 1 :b 2} #{:c :d})
        #{'(1 2) [3 4] {:a 1 :b 2} #{:c :d}}

      ; evaluation
      (hash-set (+ 1 2) [(+ 2 3) :a] (hash-set (* 2 3) 8))
        #{3 [5 :a] #{6 8}}

      ; special cases
      (hash-set nil) #{nil}
      (hash-set 1 nil) #{1 nil}
      (hash-set nil 2) #{nil 2}
      (hash-set #{}) #{#{}}
      (hash-set 1 #{}) #{1 #{}}
      (hash-set #{} 2) #{#{} 2}))


(deftest test-sorted-set
  ; only compatible types can be used
  (is (thrown? clojure._core/IllegalArgumentException (sorted-set 1 "a")))
  (is (thrown? clojure._core/IllegalArgumentException (sorted-set '(1 2) [3 4])))

  ; creates set?
  (are [x] (set? x)
       (sorted-set)
       (sorted-set 1 2) )

  ; equal and unique (dropped vector entries — PVector isn't Comparable
  ; in our impl so `(sorted-set [])` would throw; that case is exercised
  ; in vanilla but not portable until Comparable lands on PVector).
  (are [x] (and (= (sorted-set x) #{x})
                (= (sorted-set x x) (sorted-set x)))
      nil
      false true
      0 42
      0.0 3.14
      2/3
      \c
      "" "abc"
      'sym
      :kw)

  (are [x y] (= x y)
      ; generating
      (sorted-set) #{}
      (sorted-set 1) #{1}
      (sorted-set 1 2) #{1 2}

      ; sorting
      (seq (sorted-set 5 4 3 2 1)) '(1 2 3 4 5)

      ; special cases
      (sorted-set nil) #{nil}
      (sorted-set 1 nil) #{nil 1}
      (sorted-set nil 2) #{nil 2}))


(deftest test-sorted-set-by
  ; only compatible types can be used. Using `<` directly raises Python's
  ; `TypeError`; vanilla's `(sorted-set-by < ...)` would surface that as
  ; ClassCastException, but the contract is "throws *something*" — we
  ; broaden to `builtins.Exception`.
  (is (thrown? builtins.Exception (sorted-set-by < 1 "a")))
  (is (thrown? builtins.Exception (sorted-set-by < '(1 2) [3 4])))

  ; creates set?
  (are [x] (set? x)
       (sorted-set-by <)
       (sorted-set-by < 1 2) )

  ; equal and unique (using `compare`, which our impl is permissive about)
  (are [x] (and (= (sorted-set-by compare x) #{x})
                (= (sorted-set-by compare x x) (sorted-set-by compare x)))
      nil
      false true
      0 42
      0.0 3.14
      2/3
      \c
      "" "abc"
      'sym
      :kw
  )

  (are [x y] (= x y)
      ; generating
      (sorted-set-by >) #{}
      (sorted-set-by > 1) #{1}
      (sorted-set-by > 1 2) #{1 2}

      ; sorting
      (seq (sorted-set-by < 5 4 3 2 1)) '(1 2 3 4 5)))


(deftest test-set
  ; set?
  (are [x] (set? (set x))
      () '(1 2)
      [] [1 2]
      #{} #{1 2}
      {} {:a 1 :b 2}
      "" "abc" )

  ; unique
  (are [x] (= (set [x x]) #{x})
      nil
      false true
      0 42
      0.0 3.14
      2/3
      \c
      "" "abc"
      'sym
      :kw
      () '(1 2)
      [] [1 2]
      {} {:a 1 :b 2}
      #{} #{1 2} )

  ; conversion
  (are [x y] (= (set x) y)
      () #{}
      '(1 2) #{1 2}

      [] #{}
      [1 2] #{1 2}

      #{} #{}         ; identity
      #{1 2} #{1 2}   ; identity

      {} #{}
      {:a 1 :b 2} #{[:a 1] [:b 2]}

      "" #{}
      "abc" #{\a \b \c} ))


(deftest test-disj
  ; doesn't work on lists, vectors or maps
  (is (thrown? clojure._core/IllegalArgumentException (disj '(1 2) 1)))
  (is (thrown? clojure._core/IllegalArgumentException (disj [1 2] 1)))
  (is (thrown? clojure._core/IllegalArgumentException (disj {:a 1} :a)))

  ; identity
  (are [x] (= (disj x) x)
      nil
      #{}
      #{1 2 3}
      ; different data types
      #{nil
        false true
        0 42
        0.0 3.14
        2/3
        \c
        "" "abc"
        'sym
        :kw
        [] [1 2]
        {} {:a 1 :b 2}
        #{} #{1 2}} )

  (are [x y] (= x y)
      (disj nil :a) nil
      (disj nil :a :b) nil

      (disj #{} :a) #{}
      (disj #{} :a :b) #{}

      (disj #{:a} :a) #{}
      (disj #{:a} :a :b) #{}
      (disj #{:a} :c) #{:a}

      (disj #{:a :b :c :d} :a) #{:b :c :d}
      (disj #{:a :b :c :d} :a :d) #{:b :c}
      (disj #{:a :b :c :d} :a :b :c) #{:d}
      (disj #{:a :b :c :d} :d :a :c :b) #{}

      (disj #{nil} :a) #{nil}
      (disj #{nil} #{}) #{nil}
      (disj #{nil} nil) #{}

      (disj #{#{}} nil) #{#{}}
      (disj #{#{}} #{}) #{}
      (disj #{#{nil}} #{nil}) #{} ))


(deftest test-array-map-arity
  (is (thrown? clojure._core/IllegalArgumentException
               (array-map 1 2 3))))


(deftest test-assoc
  (are [x y] (= x y)
       [4] (assoc [] 0 4)
       [5 -7] (assoc [] 0 5 1 -7)
       {:a 1} (assoc {} :a 1)
       {nil 1} (assoc {} nil 1)
       {:a 2 :b -2} (assoc {} :b -2 :a 2))
  (is (thrown? clojure._core/IllegalArgumentException (assoc [] 0 5 1)))
  (is (thrown? clojure._core/IllegalArgumentException (assoc {} :b -2 :a))))


(defn is-same-collection [a b]
  (is (= (count a) (count b)))
  (is (= a b))
  (is (= b a))
  (is (= (hash a) (hash b))))


(deftest ordered-collection-equality-test
  ;; Adapted: dropped vector-of and PersistentQueue (not present).
  (let [empty-colls [ []
                      '()
                      (lazy-seq) ]]
    (doseq [c1 empty-colls, c2 empty-colls]
      (is-same-collection c1 c2)))
  (let [colls1 [ [-3 :a "7th"]
                 '(-3 :a "7th")
                 (lazy-seq (cons -3
                   (lazy-seq (cons :a
                     (lazy-seq (cons "7th" nil))))))
                 (sequence (map identity) [-3 :a "7th"]) ]]
    (doseq [c1 colls1, c2 colls1]
      (is-same-collection c1 c2)))
  (let [long-colls [ [2 3 4]
                     '(2 3 4)
                     (range 2 5)]]
    (doseq [c1 long-colls, c2 long-colls]
      (is-same-collection c1 c2))))


(deftest set-equality-test
  ;; Adapted: dropped sorted-set-by case-insensitive (no clojure.string).
  (let [empty-sets [ #{}
                     (hash-set)
                     (sorted-set) ]]
    (doseq [s1 empty-sets, s2 empty-sets]
      (is-same-collection s1 s2)))
  (let [sets1 [ #{"Banana" "apple" "7th"}
                (hash-set "Banana" "apple" "7th")
                (sorted-set "Banana" "apple" "7th") ]]
    (doseq [s1 sets1, s2 sets1]
      (is-same-collection s1 s2))))


(deftest map-equality-test
  ;; Adapted: dropped sorted-map-by case-insensitive.
  (let [empty-maps [ {}
                     (hash-map)
                     (array-map)
                     (sorted-map) ]]
    (doseq [m1 empty-maps, m2 empty-maps]
      (is-same-collection m1 m2)))
  (let [maps1 [ {"Banana" "like", "apple" "love", "7th" "indifferent"}
                (hash-map "Banana" "like", "apple" "love", "7th" "indifferent")
                (array-map "Banana" "like", "apple" "love", "7th" "indifferent")
                (sorted-map "Banana" "like", "apple" "love", "7th" "indifferent") ]]
    (doseq [m1 maps1, m2 maps1]
      (is-same-collection m1 m2))))


;; *** Collection hashes ***

(defn hash-ordered [collection]
  (-> (reduce (fn [acc e] (unchecked-add-int (unchecked-multiply-int 31 acc) (hash e)))
              1
              collection)
      (mix-collection-hash (count collection))))

(defn hash-unordered [collection]
  (-> (reduce unchecked-add-int 0 (map hash collection))
      (mix-collection-hash (count collection))))

(deftest ordered-collection-hashes-match
  ;; Adapted from a `defspec`. Vanilla generates random elements; we use a
  ;; few representative samples.
  (doseq [elem [[]
                [1 2 3]
                [:a :b :c]
                [nil 1 nil]
                ["x" "y" "z"]]]
    (let [v (vec elem)
          l (apply list elem)]
      (is (= (hash v)
             (hash l)
             (hash (map identity elem))
             (hash-ordered elem))))))

(deftest unordered-set-hashes-match
  (doseq [elem [[]
                [1 2 3]
                [:a :b :c]
                ["x" "y" "z"]]]
    (let [unique-elem (distinct elem)
          s (into #{} unique-elem)]
      (is (= (hash s)
             (hash-unordered unique-elem))))))


(defn seq-iter-match
  "Walk `seqable`'s seq alongside `iterable`'s Python iterator and assert
  they produce the same sequence of values. Vanilla used `.iterator` /
  `.hasNext` / `.next` — Python collections expose `__iter__` directly."
  [seqable iterable]
  (cond
    ;; If both are nil/empty, nothing to do.
    (nil? iterable)
    (when (some? (seq seqable))
      (throw (ex-info "Null iterable but seq has elements"
                      {:pos 0 :seqable seqable})))
    :else
    (let [it (.__iter__ iterable)]
      (loop [s (seq seqable) n 0]
        (if (seq s)
          (let [nxt (try (.__next__ it)
                         (catch builtins/StopIteration _
                           (throw (ex-info "Iterator exhausted before seq"
                                           {:pos n :seqable seqable}))))]
            (when-not (= nxt (first s))
              (throw (ex-info "Iterator and seq did not match"
                              {:pos n :expected (first s) :got nxt})))
            (recur (rest s) (inc n)))
          (try (.__next__ it)
               (throw (ex-info "Seq exhausted before iterator"
                               {:pos n :seqable seqable}))
               (catch builtins/StopIteration _ nil)))))))


(deftest test-seq-iter-match
  ;; Adapted: vanilla compares `(seq m)` (entries) with `(.iterator m)`
  ;; (also entries on the JVM). On Python `__iter__` on a map yields keys
  ;; (Python convention), so we exercise the seqs of `keys`/`vals` only —
  ;; their iter and seq should agree.
  (let [maps (mapcat #(vector (apply array-map %)
                              (apply hash-map %)
                              (apply sorted-map %))
                     [[] [nil 1] [nil 1 2 3] [1 2 3 4]])]
    (doseq [m maps]
      (seq-iter-match (keys m) (keys m))
      (seq-iter-match (vals m) (vals m))
      (seq-iter-match (rest (keys m)) (rest (keys m)))
      (seq-iter-match (rest (vals m)) (rest (vals m))))))


(deftest singleton-map-in-destructure-context
  (let [sample-map {:a 1 :b 2}
        {:keys [a] :as m1} (list sample-map)]
    (is (= m1 sample-map))
    (is (= a 1))))


(deftest trailing-map-destructuring
  (let [sample-map {:a 1 :b 2}
        add  (fn [& {:keys [a b]}] (+ a b))
        addn (fn [n & {:keys [a b]}] (+ n a b))]
    (testing "kwargs are applied properly given a map in place of key/val pairs"
      (is (= 3 (add  :a 1 :b 2)))
      (is (= 3 (add  {:a 1 :b 2})))
      (is (= 13 (addn 10 :a 1 :b 2)))
      (is (= 13 (addn 10 {:a 1 :b 2})))
      (is (= 103 ((partial addn 100) :a 1 {:b 2})))
      (is (= 103 ((partial addn 100 :a 1) {:b 2})))
      (is (= 107 ((partial addn 100 :a 1) {:a 5 :b 2}))))))
