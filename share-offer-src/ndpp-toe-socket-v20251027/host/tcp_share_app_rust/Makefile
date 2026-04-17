# ---------- 用户可调整 ----------
RUST_BIN := target/debug/rust_tcp_share_forward_app
# ---------------------------------

.PHONY: all build run test clean release

all: build

build:
	@echo "→ Building Rust project"
	cargo build

run: build
	@echo "→ Running $(RUST_BIN)"
	$(RUST_BIN)

test:
	@echo "→ Running tests"
	cargo test

release:
	@echo "→ Building release binary"
	cargo build --release

clean:
	@echo "→ Cleaning Rust build"
	cargo clean
