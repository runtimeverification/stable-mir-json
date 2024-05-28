#!/bin/sh
"$(rustup +nightly which rustc)" -vV | grep '^host' | grep -o '[^: ]*$'
