;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

;;
;; Python-friendly port of clojure.string. Only the functions whose
;; semantics carry over without StringBuilder/Pattern/Matcher interop
;; are included so far.

(ns ^{:doc "Clojure String utilities."
      :author "Stuart Sierra, Stuart Halloway, David Liebke"}
  clojure.string
  (:refer-clojure :exclude [replace reverse]))

(defn reverse
  "Returns s with its characters reversed."
  {:added "1.2"}
  [s]
  (clojure.lang.RT/str-reverse s))

(defn upper-case
  "Converts string to all upper-case."
  {:added "1.2"}
  [s]
  (clojure.lang.RT/str-upper s))

(defn lower-case
  "Converts string to all lower-case."
  {:added "1.2"}
  [s]
  (clojure.lang.RT/str-lower s))

(defn capitalize
  "Converts first character of the string to upper-case, all other
  characters to lower-case."
  {:added "1.2"}
  [s]
  (clojure.lang.RT/str-capitalize s))

(defn trim
  "Removes whitespace from both ends of string."
  {:added "1.2"}
  [s]
  (clojure.lang.RT/str-trim s))

(defn triml
  "Removes whitespace from the left side of string."
  {:added "1.2"}
  [s]
  (clojure.lang.RT/str-triml s))

(defn trimr
  "Removes whitespace from the right side of string."
  {:added "1.2"}
  [s]
  (clojure.lang.RT/str-trimr s))

(defn blank?
  "True if s is nil, empty, or contains only whitespace."
  {:added "1.2"}
  [s]
  (if (nil? s)
    true
    (clojure.lang.RT/str-blank? s)))

(defn starts-with?
  "True if s starts with substr."
  {:added "1.8"}
  [s substr]
  (clojure.lang.RT/str-starts-with? s substr))

(defn ends-with?
  "True if s ends with substr."
  {:added "1.8"}
  [s substr]
  (clojure.lang.RT/str-ends-with? s substr))

(defn includes?
  "True if s includes substr."
  {:added "1.8"}
  [s substr]
  (clojure.lang.RT/str-includes? s substr))

(defn split
  "Splits string on a regex."
  {:added "1.2"}
  ([s re]
   (clojure.lang.RT/str-split s re))
  ([s re limit]
   (clojure.lang.RT/str-split-limit s re limit)))

(defn split-lines
  "Splits s on \\n or \\r\\n."
  {:added "1.2"}
  [s]
  (clojure.lang.RT/str-split-lines s))

(defn join
  "Returns a string of all elements in coll, as returned by (seq coll),
  separated by an optional separator."
  {:added "1.2"}
  ([coll] (apply str coll))
  ([separator coll]
   (clojure.lang.RT/str-join (str separator) coll)))

(defn replace
  "Replaces all instances of match with replacement in s."
  {:added "1.2"}
  [s match replacement]
  (clojure.lang.RT/str-replace s match replacement))

(defn replace-first
  "Replaces the first instance of match with replacement in s."
  {:added "1.2"}
  [s match replacement]
  (clojure.lang.RT/str-replace-first s match replacement))

(defn index-of
  "Returns the index of substr in s, or nil if not found."
  {:added "1.8"}
  ([s substr]
   (clojure.lang.RT/str-index-of s substr))
  ([s substr from-index]
   (clojure.lang.RT/str-index-of-from s substr from-index)))
