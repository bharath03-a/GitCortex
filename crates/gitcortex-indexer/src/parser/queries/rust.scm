; ── gitcortex Rust query file ─────────────────────────────────────────────────
; Reference queries used during development / debugging.
; The production parser in rust.rs uses cursor-based tree walking so it can
; track parent context (scope path, impl type) that flat queries cannot
; easily express.

; Free-standing functions
(function_item
  name: (identifier) @name) @function

; Structs
(struct_item
  name: (type_identifier) @name) @struct

; Enums
(enum_item
  name: (type_identifier) @name) @enum

; Traits
(trait_item
  name: (type_identifier) @name) @trait

; Modules
(mod_item
  name: (identifier) @name) @mod

; Constants and statics
(const_item  name: (identifier) @name) @constant
(static_item name: (identifier) @name) @static

; Type aliases
(type_item
  name: (type_identifier) @name) @type_alias

; Macro definitions
(macro_definition
  name: (identifier) @name) @macro

; Inherent impl (no trait)
(impl_item
  type: (_) @type_name
  !trait) @impl_inherent

; Trait impl
(impl_item
  trait: (_) @trait_name
  type:  (_) @type_name) @impl_trait

; Simple identifier call expressions
(call_expression
  function: (identifier) @callee) @call
