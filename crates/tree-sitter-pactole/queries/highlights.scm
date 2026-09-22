; Comments
(comment) @comment

; Directive keywords
"open" @keyword
"close" @keyword
"commodity" @keyword
"balance" @keyword
"include" @keyword

; Transaction status flag (* ! ?)
(status) @keyword.operator

; Dates
(date) @constant.numeric.date

; Numbers
(number) @constant.numeric
(tolerance) @constant.numeric

; Strings (payee, narration, quoted metadata values)
(string) @string
(path) @string.special.path

; Accounts, e.g. Assets:Checking
(account) @variable.other.member

; Commodity / currency codes, e.g. EUR
(commodity_name) @type

; Tags and links
(tag) @tag
(link) @label

; References, e.g. (CB-00123)
(reference) @string.special

; Metadata keys, e.g. `note:`
(key) @property

; Punctuation
":" @punctuation.delimiter
["=" "~"] @operator
