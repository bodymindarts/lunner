build:
	cargo build
test:
	RUST_BACKTRACE=full cargo watch -s 'cargo test -- --nocapture'

integration: build
	bats -t -r test/integration

test-in-ci:
	cargo clippy --all-features
	cargo test --all-features --verbose --locked

.PHONY: test
