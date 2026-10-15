; Symbol and reference queries for Pactole.
;
; Payee declarations define journal symbols. Transaction payees refer to
; those symbols, even though validation is performed by pactole-core rather
; than by Tree-sitter.
(payee_declaration (string) @local.definition)
(transaction (payee (string) @local.reference))
