; Comments
(comment) @comment

; Negation prefix (!)
(negation) @operator

; Directory separators
(directory_separator) @punctuation.delimiter
(directory_separator_escaped) @string.escape

; Wildcards
(wildcard_char_single) @string.special    ; ?
(wildcard_chars) @string.special          ; *
(wildcard_chars_allow_slash) @string.special ; **

; Bracket expressions
(bracket_expr
  "[" @punctuation.bracket
  "]" @punctuation.bracket)

(bracket_negation) @operator

(bracket_range
  "-" @operator)

(bracket_char_class) @string.special.symbol

; Pattern characters
(pattern_char) @string
(pattern_char_escaped) @string.escape
(bracket_char) @string
(bracket_char_escaped) @string.escape

; Overall pattern
(pattern) @string
