RUST_DIR=${CURDIR}/deps/rust
BUILD_DIR=${RUST_DIR}/rust/build
INSTALL_DIR=${RUST_DIR}/rust/install
TEMP_DIR=${RUST_DIR}/rust/temp
RUST_BRANCH=smir_serde
TOOLCHAIN_NAME=smir_serde

default: clone build set_toolchain

clean:
	rm -rf "${BUILD_DIR}" "${INSTALL_DIR}" "${TEMP_DIR}"

clone:
	git clone https://github.com/rust-lang/rust "${BUILD_DIR}"
	cd "${BUILD_DIR}"; git checkout "${BRANCH}"

#
# build process for linking against custom rustc is involved
# 1. core rust compiler must be built (install handles this)
# 2. rustc-dev component must be installed (install does _not_ handle, must be done manually)
# 3. HACK: we copy required libraries to the libdir
#
#    Installer assumes prefix /usr/local (so installed libs will be picked up by ldconfig)
#    Since install at an uncommon prefix, we manually copy foreign runtime libs to rustlib dir,
#    so that cargo will pick them up by default
#
#    Due to hack, we _must_ run tools via cargo run or, e.g., manually set up LD_LIBRARY_PATH
#
build: ${BUILD_DIR}
	cd "${BUILD_DIR}"; ./x.py install --set "install.prefix=${INSTALL_DIR}" --set "install.sysconfdir=."
	cd "${BUILD_DIR}"; ./x.py dist rustc-dev
	mkdir -p "${TEMP_DIR}"
	cd "${BUILD_DIR}"; tar xf ./build/dist/rustc-dev*tar.gz -C "${TEMP_DIR}"
	${TEMP_DIR}/*/install.sh --prefix="${INSTALL_DIR}" --sysconfdir="${INSTALL_DIR}"
	cd "${INSTALL_DIR}/lib"; cp libLLVM* rustlib/*/lib/

set_toolchain: ${INSTALL_DIR}/lib
	rustup toolchain link "${TOOLCHAIN_NAME}" "${INSTALL_DIR}"
	rustup override set "${TOOLCHAIN_NAME}'
