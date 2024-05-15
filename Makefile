RUST_DIR=${CURDIR}/deps/rust
RUST_SRC=${RUST_DIR}/src
RUST_INSTALL=${RUST_DIR}/install
TEMP_DIR=${RUST_DIR}/temp
RUST_REPO=https://github.com/sskeirik/rust
RUST_BRANCH=smir_serde
TOOLCHAIN_NAME=smir_serde

default: rust_clone rust_build rust_set_toolchain build

build:
	cargo build

clean:
	rm -rf "${RUST_SRC}" "${RUST_INSTALL}" "${TEMP_DIR}"

# NOTE: a deeper clone depth is needed for the build process
rust_clone:
	git clone --depth 50 --single-branch --branch "${RUST_BRANCH}" "${RUST_REPO}" "${RUST_SRC}"

#
# build process for linking against custom rustc is involved
# 1. core rust compiler must be built (install handles this)
# 2. rustc-dev component must be installed (install does _not_ handle, must be done manually)
# 3. HACK: we copy required libraries to the libdir
#
#    See implications of hack in README
#
rust_build: ${RUST_SRC}
	cd "${RUST_SRC}"; ./x.py install --set "install.prefix=${RUST_INSTALL}" --set "install.sysconfdir=." compiler/rustc library/std
	cd "${RUST_SRC}"; ./x.py dist rustc-dev
	mkdir -p "${TEMP_DIR}"
	cd "${RUST_SRC}"; tar xf ./build/dist/rustc-dev*tar.gz -C "${TEMP_DIR}"
	${TEMP_DIR}/*/install.sh --prefix="${RUST_INSTALL}" --sysconfdir="${RUST_INSTALL}"
	cd "${RUST_INSTALL}/lib"; cp libLLVM* rustlib/*/lib/

rust_set_toolchain: ${RUST_INSTALL}/lib
	rustup toolchain link "${TOOLCHAIN_NAME}" "${RUST_INSTALL}"
	rustup override set "${TOOLCHAIN_NAME}"
