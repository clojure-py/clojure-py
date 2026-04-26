# Ants demo (Tk port of Rich Hickey's 2009 Swing demo)

Run from project root:

    python -m clojure examples/ants/ants.clj

You should see an 80×80 window. Ants leave the central home, search for
food (red squares), pick it up, and trail pheromone (green) back. Other
ants follow the trail. Pheromone evaporates over time.

This is a strict port of <https://gist.github.com/michiakig/1093917>
(Rich Hickey's 2009 Clojure Concurrency demo). The only differences from
the original are (a) Tk method calls instead of Swing, (b) `(:import
[tkinter ...])` at the top, and (c) `defrecord Cell` in place of the
original's `defstruct cell` (clojure-py does not implement defstruct).
The simulation, rules, constants, agent topology, and STM patterns are
identical.

Press Ctrl-C in the terminal to quit.
