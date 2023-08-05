.PHONY: init readme

init:
	cargo install cargo-readme

readme:
	cargo readme > README.md
