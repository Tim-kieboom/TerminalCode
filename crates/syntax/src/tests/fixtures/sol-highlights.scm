(line_comment) @comment
(block_comment) @comment

;; The editor lets the FIRST pattern win where two capture the same text (Zed keeps the last),
;; so what is more specific comes before the generic (identifier) and (type_identifier).
(call_expression
  function: (identifier) @function)
(call_expression
  function: (type_identifier) @constructor)
(call_expression
  function: (primitive_type) @function)

(keyword) @keyword
(storage_modifier) @keyword
(constant) @constant
(primitive_type) @type
(lifetime) @label

(attribute) @attribute

(string) @string
(char_literal) @string
(escape_sequence) @string.escape
(string_start) @string
(format_string) @string

(number) @number

(type_identifier) @type
(identifier) @variable

(operator) @operator

["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["," "." ":" ";"] @punctuation.delimiter
["<" ">"] @operator
