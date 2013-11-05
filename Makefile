bin/pyrite: bin src/pyrite.rs src/core.rs src/shapes.rs
	rustc -O -o bin/pyrite src/pyrite.rs

bin:
	mkdir bin

lint:
	rustc -O --no-trans src/pyrite.rs