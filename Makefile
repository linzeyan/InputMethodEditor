# InputMethodEditor — a Chinese input method for Windows (TSF), built on libchewing.
#
#   make build    cross-build dist/InputMethodEditor and its zip
#   make test     run the tests that run on this machine
#   make check    lint and tests; run before push or tag
#
# Target names follow ~/git/dev_docs/project-conventions/references/makefile.md.
# release.sh lives in docs/scripts (a symlink into ~/git/dev_docs/IME).

SHELL   := /bin/bash
BASH5   := /opt/homebrew/bin/bash
VERSION ?=

# llvm-mingw is not on PATH on the Mac; CI puts its own there. Only the cross
# builds get it: its clang would shadow the system's in a native build.
LLVM_MINGW ?= $(HOME)/.local/opt/llvm-mingw-20260922/bin
CROSS      := PATH="$(LLVM_MINGW):$$PATH"
DIST       := $(CROSS) cargo xtask dist --release --target gnullvm

# Local builds skip the shipping LTO: after a change in libchewing the four
# release builds take 35 s instead of 44 s (M5 Max, 18 cores; a cold build and
# a change in the TIP alone take the same either way). CI sets CI, so there
# the profile stays as Cargo.toml has it, which is what ships.
ifndef CI
export CARGO_PROFILE_RELEASE_LTO           ?= false
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS ?= 16
endif

# The one pin for rustfmt and clippy, which run here and in pre-push, not in
# CI. Bump on purpose, together with the fixes the new lints ask for.
LINT_RUST := 1.97.1

# vendor/ is upstream's code in upstream's style: it is neither formatted nor
# linted here.
CRATES := chewing_tip chewing_tip_core tsfreg ime-settings xtask

.DEFAULT_GOAL := help
.PHONY: help setup build build-arm64 msi test lint fmt check release

help: ## Show this help
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z0-9_-]+:.*## / {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

setup: ## Add the Windows targets and the pre-push hook
	rustup target add x86_64-pc-windows-gnullvm i686-pc-windows-gnullvm
	rustup target add --toolchain $(LINT_RUST) x86_64-pc-windows-gnullvm
	git config core.hooksPath .githooks

build: ## Cross-build dist/InputMethodEditor and its zip
	$(DIST)

build-arm64: ## Cross-build dist/InputMethodEditor-arm64: ARM64 in place of x64
	$(DIST) --arm64

msi: ## Cross-build dist/InputMethodEditor.msi as well, as CI ships it (needs wixl)
	$(DIST) --msi

# The other crates' tests are cross-built and run on a Windows VM
# (docs/scripts/vm-check.sh). These two hold every `auto` row of
# docs/regressions.md.
test: ## Tests that run on this machine: libchewing and xtask
	cargo test -p chewing --lib
	cargo test -p xtask

# Clippy lints the Windows build, which is what the code is written for. The
# last line lets .githooks/pre-push skip a tree that already passed.
lint: ## Format and lint checks (run here and in pre-push, not in CI)
	cargo +$(LINT_RUST) fmt --check $(CRATES:%=-p %)
	$(CROSS) RC=llvm-rc RUSTFLAGS=-Ctarget-feature=+crt-static cargo +$(LINT_RUST) clippy --no-deps \
		$(CRATES:%=-p %) --target x86_64-pc-windows-gnullvm --all-targets -- -D warnings
	@git diff --quiet HEAD && git rev-parse 'HEAD^{tree}' >"$$(git rev-parse --git-path lint-ok)" || true

fmt: ## Format in place, with the same rustfmt lint checks against
	cargo +$(LINT_RUST) fmt $(CRATES:%=-p %)

check: lint test ## Lint and tests; run before push or tag

# Depends on check so every regression test (docs/regressions.md) passes
# before anything ships.
release: check ## Bump to VERSION, push main, then tag it for CI to publish; needs VERSION=x.y.z
	@test -n "$(VERSION)" || { echo "VERSION is required: make release VERSION=x.y.z" >&2; exit 2; }
	$(BASH5) docs/scripts/release.sh $(subst ., ,$(VERSION)) </dev/null
