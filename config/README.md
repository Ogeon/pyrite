#Grammar

```
statement: [include | assign]
include: 'include' string
assign: path '=' value
value: [object | number | string | list]
object: [path | '{' assign* '}' | extension]
extension: path ['{' assign* '}' | '(' value,* ')']
path: ['root.' | ident '.' ]? ident.+
number: digit+ ['.' digit+]?
string: '"' ['\' char | char ]* '"'
list: '[' value,* ']'

reserved: 'include', 'root'
```