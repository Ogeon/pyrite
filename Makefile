bin/pyrite: bin src/*
	rustc -O -o bin/pyrite src/pyrite.rs

bin:
	mkdir bin

lint:
	rustc -O --no-trans src/pyrite.rs