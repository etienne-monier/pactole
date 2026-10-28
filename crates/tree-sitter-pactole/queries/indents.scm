; Indentation queries for Pactole.
;
; Transaction contents, directive properties and postings are indented below
; their header. Capturing the transaction itself is important for the empty
; line where the first posting is about to be entered: there is no `posting`
; node in the syntax tree yet at that point.
(transaction) @indent
(property) @indent
(posting) @indent
