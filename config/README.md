# Pyrite Project Configuration Language

This is a mix of JSON style configuration and an inheritance hierarchy that
comes together as a relatively simple, yet powerful, configuration language.
Its primary features are:

 * Creating template configurations and extending or referring to them.
 * Both static and context aware dynamic decoding.
 * Splitting configuration into multiple files, using the `include ...` statement.

_Note: It's very experimental and unstable, so expect both dragons and missing
laundry._

# Syntax

The syntax is quite simple. A configuration file consists of a bunch of
statements, and they can only do two thing: include or assign.

The include statement includes another configuration file into the current
one. It can either be used to completely merge the two files as if they were
one (`include "path/to/file"`), or to include the file under a namespace
(`include "path/to/file" as some_name`). The latter option isolates the
included file in its own little sandbox where it's unable to access anything
you have defined outside of it.

Assignments are the actual configuration part. They consists of a path, an `=`
and a value: `path.to.something = ...`. Each part of the path will be created
automatically if they are not defined beforehand, so there is no need to
explicitly define `path` and `path.to`. There are thee basic kinds of values:
object, list and primitive values. Let's take a closer look at them.

## Objects

These are basic key-value structures and you could say that a configuration
file is nothing but one big object. An object is defined using `{ ... }`, like
so:

```
a = {
    a = ...
    b = ...
    c = ...
}
```

This will create an object `a`, containing the values `a`, `b` and `c`.

An other way of doing the same thing is to refer directly to the child values
of `a` like this:

```
a.a = ...
a.b = ...
a.c = ...
```

This can, for example, come in handy if the object hierarchies are very deep,
and the two styles can even be combined.

An other thing that objects can do is extending other objects. You can create
a sort of template or prototype object that contains the most common values
and then extend it:

```
basic_sphere = {
    position = {
        x = 0
        y = 0
        z = 0
    }
    radius = 1
}

big_sphere = basic_sphere {
    radius = 10
}

distant_sphere = basic_sphere {
    position.y = 100
}
```

An extended object will contain all the values from its base object and allow
them to be locally overwritten, which is useful to minimize repetitions.
Extending has even been taken a step further, with a function like syntax that
makes things like this possible:

```
basic_sphere = {
    position = Point(0, 0, 0)
    radius = 1
}

big_sphere = basic_sphere {
    radius = 10
}

distant_sphere = basic_sphere {
    position.y = 100
}
```

This requires `Point` to have a predefined argument list that maps to `x`, `y`
and `z`, so it's basically the same as the previous example. More on that
later.

An object can only extend an other object, but just referring to other types
is allowed. This is forbidden:

```
a = 10
b = a {
    c = 0
}
```

...since a number can't contain `c = 0`, but this is completely valid:

```
a = 10
b = a
```

`b` will then be considered a number and have whatever value `a` has.

A reference path is always global, unless they are prefixed with `self`, as
in:

```
position = {
    x = 5
    y = 3
    z = self.x
}
```

## Lists

Lists are defined as `[ ... ]` and may contain 0 or more arbitrary elements.
Each element is separated by `,` and it may look like this:

```
a = [10, { b = 5 }, [2]]
```

List elements can not be referred to in paths, so they can't be used as
templates.

## Primitive Values

The types of primitive values that are supported are numbers (`-5`, `42`,
`3.14`, etc.) and strings (`"Hello!"`). Nothing more, nothing less.

## Unknown Values

There are some situations where a value is referred to, but not defined. There
values exists, but they don not have any known type and cannot be used in a
meaningful way. They should be unusual and should only appear if a value is
set to refer to another, but that other value is left undefined.

# Parsing And Decoding

Configuration files are parsed through the `Parser` type and can be done
incrementally. This means that multiple files and arbitrary strings can be
combined on the fly. The parsed configuration can then be accessed through
`my_parse.root()`, which returns an `Entry` that in this case represents
everything that has been parsed.

## Static Decoding

A type may implement the `FromEntry` trait, which allows it to be decoded from
an `Entry`, which is fairly similar to how Serde, rustc-serialize and other
decoders works. The `Entry` type has a `decode` method that will return a
decoded values if it's successful:

```rust
let mut config = Parser::new();
if let Err(e) = config.read_file("path/to/config") {
    panic!("failed to parse config: {}", e);
}

//This assumes that "a" exists and will panic otherwise
match config.root().get("a").decode() {
    Ok(num) => assert_eq!(42, num),
    Err(e) => println!("Failed to decode `a` number: {}", e)
}
```

## Dynamic Decoding

You may think that "static decoding" is an odd name, considering that decoding
may still produce runtime errors, but it's still more static than dynamic
decoding. There are situations where decoding is more context dependent than
what static decoding can give you. Imagine, for example, that you want to
decode decode a trait object `Box<MyTrait>`, but that would be very
problematic. What concrete type is it? What about user defined types? That's
where dynamic decoding is useful.

The `Entry` type has a second decoding method, called `dynamic_decode` that is
similar to `decode`, but there is no `FromEntry` trait that decides if a type
can be decoded or not. There are, instead, a collection of predefined
configuration entries, called "the prelude", that has associated decoders.
These entries must then be referred to or extended, to allow dynamic decoding
of an other entry. Here is an example that may make it a bit easier to
understand:

```rust
fn decode_rgb(entry: Entry) -> Result<Color, String> { ... }
fn decode_cmyk(entry: Entry) -> Result<Color, String> { ... }

let mut prelude = Prelude::new();
{
    //Create a prelude object for our color types
    let mut color = prelude.object("Color".into());

    //Create an RGB object and attach a decoder
    color.object("Rgb".into()).add_decoder(decode_rgb);

    //Create a CMYK object and attach a decoder
    color.object("Cmyk".into()).add_decoder(decode_cmyk);
}

let mut config = prelude.into_parser();
if let Err(e) = config.read_file("path/to/config") {
    panic!("failed to parse config: {}", e);
}

match config.root().get("some_color").dynamic_decode::<Color>() {
    Ok(color) => println!("the color is {:?}", color),
    Err(e) => println!("Failed to decode the color: {}", e)
}
```

The configuration file may look like this:

```
some_color = Color.Rgb {
    red = 255
    green = 0
    blue = 255
}
```

Doing the same thing with static decoding would require a good amount of
guessing, based on the content of the entry, and the difficulty of that may
range from "no problem" to "OHMYWHATAMIDOINGWITHMYLIFE?!". This feature is,
however, both a blessing and a curse. The good part is that you can decode
things based on its context, but the bad part is that you need context to
decode things.

## More About The Prelude

This "prelude" thing allows you to add predefined values that will be
available everywhere from within the configuration. It's a bit like a standard
library. You have seen how it allows decoders to be attached to values, but
it's also the place where argument lists can be defined. These were mentioned
in the part about objects and allows them to be extended using the `path(arg1,
arg2, ...)` syntax. An argument list is added when the prelude object is
created, just like we added decoders:

```
let mut prelude = Prelude::new();
prelude.object("Point".into()).arguments(vec!["x".into(), "y".into(), "z".into()]);
```

This allows `Point` to be used like this: `a = Point(1, 2, 3)`.

The values in the prelude lives in their own little box, meaning that they
cannot be navigated to or used directly, but only referred to. This makes it
possible to "overwrite" them locally, but they will still shadow the local
version when referred to. This may cause some confusion and will probably
change in the future.
