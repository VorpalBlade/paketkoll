# This makefile exists to allow for an install target, since it seems
# cargo install is too basic to handle installing support files

CARGO_FLAGS ?=
DESTDIR ?=
PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
DATADIR ?= $(PREFIX)/share
BASHDIR ?= $(DATADIR)/bash-completion/completions
ZSHDIR ?= $(DATADIR)/zsh/site-functions
FISHDIR ?= $(DATADIR)/fish/vendor_completions.d
MANDIR ?= $(DATADIR)/man/man1

PROGS := target/release/paketkoll target/release/konfigkoll target/release/konfigkoll-rune target/release/xtask

all: $(PROGS)

target/release/paketkoll: build-cargo
target/release/konfigkoll: build-cargo
target/release/konfigkoll-rune: build-cargo
target/release/xtask: build-cargo

build-cargo:
	# Let cargo figure out if a build is needed
	cargo build --locked --release $(CARGO_FLAGS)

test:
	cargo test --locked --release $(CARGO_FLAGS)

install: install-paketkoll install-konfigkoll

install-paketkoll: target/release/paketkoll target/release/xtask install-dirs
	install $< $(DESTDIR)$(BINDIR)
	./target/release/xtask man --output $(DESTDIR)$(MANDIR) paketkoll
	./target/release/xtask completions --output target/completions paketkoll
	install -Dm644 target/completions/paketkoll.bash $(DESTDIR)$(BASHDIR)/paketkoll
	install -Dm644 target/completions/paketkoll.fish $(DESTDIR)$(FISHDIR)/paketkoll.fish
	install -Dm644 target/completions/_paketkoll $(DESTDIR)$(ZSHDIR)/_paketkoll


install-konfigkoll: target/release/konfigkoll target/release/konfigkoll-rune target/release/xtask install-dirs
	install $< $(DESTDIR)$(BINDIR)
	install target/release/konfigkoll-rune $(DESTDIR)$(BINDIR)
	./target/release/xtask man --output $(DESTDIR)$(MANDIR) konfigkoll
	./target/release/xtask completions --output target/completions konfigkoll
	install -Dm644 target/completions/konfigkoll.bash $(DESTDIR)$(BASHDIR)/konfigkoll
	install -Dm644 target/completions/konfigkoll.fish $(DESTDIR)$(FISHDIR)/konfigkoll.fish
	install -Dm644 target/completions/_konfigkoll $(DESTDIR)$(ZSHDIR)/_konfigkoll

install-dirs:
	install -d $(DESTDIR)$(BINDIR) $(DESTDIR)$(BASHDIR) $(DESTDIR)$(ZSHDIR) $(DESTDIR)$(FISHDIR) $(DESTDIR)$(MANDIR)

.PHONY: all build-cargo test install install-paketkoll install-konfigkoll install-dirs $(PROGS)
