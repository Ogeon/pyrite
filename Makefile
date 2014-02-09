nalgebra=lib/nalgebra/lib
png=lib/rust-png
libs=-L$(nalgebra) -L$(png)

bin/pyrite: bin src/*
	rustc $(libs) -O -o bin/pyrite src/pyrite.rs

bin:
	mkdir bin

lint:
	rustc -O --no-trans src/pyrite.rs

update:
	git submodule init
	git submodule update

deps:
	make -C lib/nalgebra
	make -C lib/rust-png -f Makefile.in