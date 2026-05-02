from setuptools import setup
from Cython.Build import cythonize

# Single Cython extension: clojure.lang. The .pyx textually includes .pxi source
# files from src/clojure/_lang/, producing one .so for the whole port.
setup(
    ext_modules=cythonize(
        ["src/clojure/lang.pyx"],
        compiler_directives={
            "language_level": "3",
            "freethreading_compatible": True,
            "boundscheck": False,
            "wraparound": False,
            "cdivision": True,
        },
    ),
)
