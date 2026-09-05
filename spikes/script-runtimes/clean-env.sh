#!/usr/bin/env bash
# The user's shell exports an Anaconda C/C++ toolchain (CC, CXX, CFLAGS with
# -march=nocona, CPPFLAGS pointing into ~/anaconda3, ...). That toolchain breaks
# mlua's vendored Luau build in two separate ways, both recorded in README.md.
# Everything that touches a C build goes through this wrapper.
exec env -u CC -u CXX -u GCC -u CFLAGS -u CXXFLAGS -u CPPFLAGS -u LDFLAGS \
  -u DEBUG_CFLAGS -u DEBUG_CXXFLAGS -u DEBUG_CPPFLAGS -u CMAKE_ARGS \
  -u CMAKE_PREFIX_PATH -u AR -u RANLIB "$@"
