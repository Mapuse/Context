include env.mk

PROFILE ?= release
TARGET  := target/$(RUST_TARGET)/$(PROFILE)/context
DESTDIR  ?=

.PHONY: all build install clean uninstall

all: build

build:
	CARGO_TARGET_DIR=$(CURDIR)/target cargo build --target $(RUST_TARGET) --profile $(PROFILE) --locked

install: build
	install -Dm755 $(TARGET) $(DESTDIR)/bin/context

uninstall:
	rm -f $(DESTDIR)/bin/context

clean:
	cargo clean
