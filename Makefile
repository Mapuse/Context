include env.mk

ROOT    := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))
PROFILE ?= release
TARGET  := target/$(RUST_TARGET)/$(PROFILE)/ctx
DESTDIR  ?=

# ── cps: external package, fetched from upstream ─────────────────────────
CPS_URL ?= https://github.com/Mapuse/CPS
CPS_DIR ?= $(HOME)/cudane-deps/cps
CPS_REF ?= c4ba21e185398558052acec3f0b4619b4e8c0678

$(CPS_DIR):
	git clone $(CPS_URL) $(CPS_DIR)
	git -C $(CPS_DIR) checkout $(CPS_REF)

.PHONY: all build deps install install-man clean uninstall

all: build

deps: $(CPS_DIR)

build: $(CPS_DIR)
	cd $(ROOT) && CARGO_TARGET_DIR=$(ROOT)target cargo build --target $(RUST_TARGET) --profile $(PROFILE) --locked

install: build install-man
	install -Dm755 $(TARGET) $(DESTDIR)/bin/ctx

install-man:
	install -d $(DESTDIR)$(PREFIX)/share/man/man1
	install -m 644 docs/ctx.1 $(DESTDIR)$(PREFIX)/share/man/man1/

uninstall:
	rm -f $(DESTDIR)/bin/ctx

clean:
	cargo clean
