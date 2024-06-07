RUST_DIR=${CURDIR}/deps/rust
RUST_SRC=${RUST_DIR}/src
RUST_ARCH=$(shell "${PWD}"/rustc_arch.sh)
RUST_BUILD_DIR=${RUST_SRC}/build/${RUST_ARCH}
RUST_INSTALL=${RUST_BUILD_DIR}/stage2
RUST_DEP_DIR=${RUST_BUILD_DIR}/stage1-rustc/${RUST_ARCH}/release/deps
TARGET=debug
TARGET_DEP_DIR=${CURDIR}/target/${TARGET}/deps
TEMP_DIR=${RUST_DIR}/temp
RUST_REPO=https://github.com/runtimeverification/rust
RUST_BRANCH=smir_serde
TOOLCHAIN_NAME=smir_serde

build: rust_build rust_set_toolchain cargo_build

setup: rust_clone

update: ${RUST_SRC}
	cd "${RUST_SRC}"; git fetch origin; git reset --hard origin/${RUST_BRANCH}

cargo_build:
	-cargo build
	cp ${RUST_DEP_DIR}/libserde-*.rmeta ${TARGET_DEP_DIR}/libserde-*.rmeta
	cp ${RUST_DEP_DIR}/libserde-*.rlib  ${TARGET_DEP_DIR}/libserde-*.rlib
	cargo build

clean:
	cd "${RUST_SRC}" && git clean -dffx
	-rm -r "${TEMP_DIR}"

# this clean removes old backup files which accumulate and lead to slow build times
prebuild_clean: ${RUST_SRC}
	-find -name '*.old' -print -delete
	-rm -r "${TEMP_DIR}"

# NOTE: a deeper clone depth is needed for the build process
rust_clone:
	git clone --depth 70 --single-branch --branch "${RUST_BRANCH}" "${RUST_REPO}" "${RUST_SRC}"


# rust_build for linking against custom rustc is involved
#
# 1. core rust compiler must be built (./x.py build/install handles this)
# 2. rustc-dev component must be installed (./x.py build/install does _not_ handle, must be done manually)
# 3. HACK(only for ./x.py install) we copy required libraries to the libdir
# 4. finally, use rustup to create custom toolchain

rust_build: ${RUST_SRC} prebuild_clean
	cd "${RUST_SRC}"; ./x.py build --stage 2 --set rust.debug-logging=true compiler/rustc library/std
	cd "${RUST_SRC}"; ./x.py dist --set rust.debug-logging=true rustc-dev
	mkdir -p "${TEMP_DIR}"
	cd "${RUST_SRC}"; tar xf ./build/dist/rustc-dev*tar.gz -C "${TEMP_DIR}"
	${TEMP_DIR}/rustc-dev*/install.sh --prefix="${RUST_INSTALL}" --sysconfdir="${RUST_INSTALL}" > "${RUST_DIR}"/rustc-dev-install.log 2>&1

rust_lib_copy:
	cd "${RUST_INSTALL}/lib"; cp libLLVM* rustlib/*/lib/

rust_set_toolchain: ${RUST_INSTALL}/lib
	rustup toolchain link "${TOOLCHAIN_NAME}" "${RUST_INSTALL}"
	rustup override set "${TOOLCHAIN_NAME}"
