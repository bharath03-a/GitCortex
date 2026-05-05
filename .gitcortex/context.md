# Codebase Map

> Branch: `feature/v0-2` · 532 definitions · SHA: `427580f01f7694aceeb662969f76c199f4fbd847`

## crates

- `pub folder crates` :1
  - `pub folder gitcortex-store` :1
    - `pub folder src` :1
      - `pub file kuzu.rs` :1
        - `pub struct KuzuGraphStore` :23
          - `pub method open` :34
          - `method conn` :59
          - `method ensure_branch` :64
          - `method apply_diff` :75
          - `method lookup_symbol` :311
          - `method find_callers` :332
          - `method find_callers_deep` :350
          - `method symbol_context` :394
          - `method list_definitions` :446
          - `method branch_diff` :462
          - `method list_all_nodes` :502
          - `method list_all_edges` :512
          - `method find_callees` :539
          - `method find_implementors` :586
          - `method trace_path` :628
          - `method list_symbols_in_range` :694
          - `method find_unused_symbols` :719
          - `method get_subgraph` :744
          - `method last_indexed_sha` :859
          - `method set_last_indexed_sha` :863
        - `constant NODE_COLS` :872
        - `function rows_to_nodes` :877
        - `function row_to_node` :888
        - `function collect_ids` :947
        - `function str_val` :961
        - `function i64_val` :973
        - `function bool_val` :983
        - `function kind_from_str` :997
        - `function edge_kind_from_str` :1018
        - `function vis_str` :1031
        - `function vis_from_str` :1039
        - `function esc` :1051
  - `pub folder gitcortex-mcp` :1
    - `pub folder src` :1
      - `pub folder cmd` :1
        - `pub file serve.rs` :1
          - `pub function run` :3
        - `pub file clean.rs` :1
          - `pub function run` :8
          - `function repo_root` :26
        - `pub file query.rs` :1
          - `pub function run` :9
          - `function repo_root` :82

## crates/gitcortex-core

- `pub folder gitcortex-core` :1
  - `pub folder src` :1
    - `pub file store.rs` :1
      - `pub struct SubGraph` :10
      - `pub struct CallersDeep` :16
      - `pub struct SymbolContext` :24
      - `pub trait GraphStore` :40
    - `pub file graph.rs` :1
      - `pub struct NodeId` :15
        - `pub method new` :18
        - `pub method as_str` :22
        - `method default` :28
        - `method fmt` :34
        - `method try_from` :42
      - `pub struct Span` :52
      - `pub struct LldLabels` :62
      - `pub struct NodeMetadata` :72
      - `pub struct Node` :101
      - `pub struct Edge` :116
      - `pub struct GraphDiff` :127
        - `pub method is_empty` :155
        - `pub method merge` :172
      - `module tests` :190
        - `function node_id_is_unique` :194
        - `function graph_diff_merge` :201
        - `function graph_diff_is_empty_on_default` :224
    - `pub file schema.rs` :1
      - `pub constant SCHEMA_VERSION` :5
      - `pub enum NodeKind` :10
        - `method fmt` :34
      - `pub enum EdgeKind` :59
        - `method fmt` :82
      - `pub enum Visibility` :100
        - `method fmt` :108
      - `pub enum SolidHint` :122
      - `pub enum DesignPattern` :138
      - `pub enum CodeSmell` :151
    - `pub file error.rs` :1
      - `pub enum GitCortexError` :11
      - `pub type_alias Result` :33
    - `pub file lib.rs` :1
      - `pub module error` :1
      - `pub module graph` :2
      - `pub module schema` :3
      - `pub module store` :4

## crates/gitcortex-core/src


## crates/gitcortex-core/src/error.rs


## crates/gitcortex-core/src/graph.rs


## crates/gitcortex-core/src/lib.rs


## crates/gitcortex-core/src/schema.rs


## crates/gitcortex-core/src/store.rs


## crates/gitcortex-indexer

- `pub folder gitcortex-indexer` :1
  - `pub folder src` :1
    - `pub file lib.rs` :1
      - `pub module differ` :1
      - `pub module indexer` :2
      - `pub module parser` :3
    - `pub file indexer.rs` :1
      - `type_alias FileIndexResult` :20
      - `pub struct IncrementalIndexer` :35
        - `pub method new` :44
        - `pub method run` :59
        - `method supported_extensions` :175
        - `method index_file` :179
        - `method should_ignore` :249
      - `function resolve_deferred` :263
      - `function build_structural_nodes` :297
      - `function build_ignorer` :420
    - `pub file differ.rs` :1
      - `pub enum FileChange` :10
        - `pub method path` :17
      - `pub struct Differ` :27
        - `pub method open` :33
        - `pub method head_sha` :40
        - `pub method changed_files` :57
    - `pub folder parser` :1
      - `pub file java.rs` :1
        - `pub struct JavaParser` :15
          - `pub method new` :20
          - `method default` :28
          - `method extensions` :34
          - `method parse` :38
        - `struct FileVisitor` :75
          - `method new` :96
          - `method text` :141
          - `method span` :145
          - `method visibility` :152
          - `method is_async` :170
          - `method modifiers_text` :176
          - `method qualified` :186
          - `method make_node` :194
          - `method collect_names` :225
          - `method visit_program` :247
          - `method visit_top_level` :255
          - `method visit_class` :265
          - `method visit_class_nested` :342
          - `method visit_interface_nested` :388
          - `method visit_interface` :427
          - `method visit_enum` :477
          - `method visit_record` :515
          - `method visit_method` :543
          - `method collect_imports` :609
          - `method extract_annotation_uses` :639
          - `method has_functional_interface_annotation` :666
          - `method extract_field_uses` :688
          - `method extract_simple_type` :697
          - `method collect_type_names` :715
          - `method walk_type_names` :721
          - `method collect_calls` :742
          - `method callee_name` :761
          - `method record_call` :781
        - `function is_builtin_java_type` :805
        - `module tests` :860
          - `function parse` :866
          - `function parse_full` :876
          - `function parses_class_and_method` :904
          - `function parses_interface` :917
          - `function parses_enum` :929
          - `function detects_extends_and_implements` :938
          - `function detects_type_annotation_uses` :950
          - `function detects_import_declaration` :961
          - `function module_node_is_emitted` :971
      - `pub file mod.rs` :1
        - `pub module go` :8
        - `pub module java` :9
        - `pub module python` :10
        - `pub module rust` :11
        - `pub module typescript` :12
        - `pub struct ParseResult` :15
        - `pub trait LanguageParser` :40
        - `pub function parser_for_path` :51
      - `pub file python.rs` :1
        - `pub struct PythonParser` :15
          - `pub method new` :20
          - `method default` :28
          - `method extensions` :34
          - `method parse` :38
        - `struct FileVisitor` :75
          - `method new` :94
          - `method text` :137
          - `method span` :141
          - `method visibility` :149
          - `method qualified` :157
          - `method make_node` :165
          - `method collect_names` :194
          - `method visit_module` :246
          - `method visit_top_level` :254
          - `method visit_function` :282
          - `method visit_class` :355
          - `method maybe_visit_constant` :494
          - `method collect_imports` :520
          - `method collect_calls` :577
          - `method callee_name` :594
          - `method record_call` :605
          - `method fn_is_async` :630
          - `method body_has_yield` :638
          - `method collect_decorators` :653
          - `method decorator_name` :662
          - `method extract_param_types` :685
          - `method extract_return_type` :705
          - `method collect_type_names` :714
          - `method walk_type_names` :720
        - `function is_builtin_type` :740
        - `module tests` :799
          - `function parse` :805
          - `function parse_full` :817
          - `function parses_free_function` :841
          - `function parses_class_and_method` :851
          - `function detects_call_edges` :872
          - `function detects_base_class_implements` :880
          - `function detects_type_annotation_uses` :890
          - `function detects_decorator_uses` :901
          - `function detects_import_statement` :911
          - `function detects_from_import_statement` :925
          - `function module_node_is_emitted` :939
          - `function async_function_flagged` :947
      - `pub file go.rs` :1
        - `pub struct GoParser` :15
          - `pub method new` :20
          - `method default` :28
          - `method extensions` :34
          - `method parse` :38
        - `struct FileVisitor` :76
          - `method new` :95
          - `method text` :154
          - `method span` :158
          - `method visibility` :166
          - `method qualified` :179
          - `method make_node` :187
          - `method collect_names` :214
          - `method collect_type_decl_names` :239
          - `method visit_source_file` :258
          - `method visit_top_level` :266
          - `method visit_function` :276
          - `method visit_method` :306
          - `method receiver_type` :340
          - `method visit_type_decl` :365
          - `method visit_const_decl` :418
          - `method collect_imports` :437
          - `method record_import_spec` :465
          - `method collect_interface_assertions` :490
          - `method collect_candidate_type_names` :533
          - `method extract_fn_type_uses` :553
          - `method extract_struct_field_uses` :598
          - `method extract_interface_methods` :633
          - `method collect_generic_bounds` :665
          - `method collect_type_idents` :697
          - `method walk_type_idents` :703
          - `method collect_calls` :722
          - `method callee_name` :749
          - `method record_call` :760
        - `function is_builtin_go_type` :784
        - `module tests` :815
          - `function parse` :821
          - `function parse_full` :831
          - `function parses_function` :844
          - `function parses_struct_and_method` :856
          - `function parses_interface` :877
          - `function go_visibility_is_uppercase` :886
          - `function detects_call_edges` :897
          - `function package_node_is_emitted` :905
          - `function detects_import_declaration` :914
          - `function detects_fn_type_uses` :928
          - `function detects_interface_assertion` :942
          - `function captures_interface_methods` :952
      - `pub file typescript.rs` :1
        - `pub struct TypeScriptParser` :21
          - `pub method new_ts` :26
          - `pub method new_tsx` :32
          - `method extensions` :58
          - `method parse` :62
        - `pub struct JavaScriptParser` :39
          - `pub method new` :44
          - `method default` :52
          - `method extensions` :68
          - `method parse` :72
        - `function parse_source` :77
        - `struct FileVisitor` :117
          - `method new` :137
          - `method text` :180
          - `method span` :184
          - `method visibility` :191
          - `method is_async` :206
          - `method qualified` :212
          - `method make_node` :220
          - `method collect_names` :247
          - `method collect_names_from_var_decl` :279
          - `method unwrap_export` :302
          - `method visit_program` :325
          - `method visit_statement` :334
          - `method visit_function` :366
          - `method visit_class` :406
          - `method visit_method` :448
          - `method visit_interface` :475
          - `method visit_type_alias` :506
          - `method visit_enum` :520
          - `method visit_namespace` :553
          - `method visit_var_decl` :575
          - `method collect_imports` :630
          - `method extract_generic_constraints` :695
          - `method extract_param_types` :712
          - `method extract_return_type_annotation` :728
          - `method extract_heritage` :739
          - `method collect_extends_names` :762
          - `method collect_implements_names` :783
          - `method extract_decorator_annotated` :801
          - `method extract_decorator_uses` :813
          - `method decorator_name` :825
          - `method collect_type_names` :847
          - `method walk_type_names` :853
          - `method collect_calls` :874
          - `method callee_name` :892
          - `method record_call` :903
        - `function is_builtin_ts_type` :927
        - `module tests` :975
          - `function parse_ts` :981
          - `function parse_ts_full` :993
          - `function parse_js` :1008
          - `function parses_ts_function` :1021
          - `function parses_ts_class_and_method` :1030
          - `function parses_ts_interface` :1051
          - `function parses_js_arrow_function` :1059
          - `function detects_ts_call_edges` :1070
          - `function detects_ts_extends_implements` :1078
          - `function detects_ts_type_annotation_uses` :1090
          - `function detects_ts_named_imports` :1101
          - `function module_node_is_emitted` :1115
      - `pub file rust.rs` :1
        - `pub struct RustParser` :17
          - `pub method new` :22
          - `method default` :30
          - `method extensions` :36
          - `method parse` :40
        - `struct FileVisitor` :81
          - `method new` :96
          - `method text` :114
          - `method field_text` :118
          - `method span` :124
          - `method visibility` :131
          - `method is_async` :146
          - `method is_unsafe` :152
          - `method is_const` :158
          - `method collect_generic_bounds` :166
          - `method collect_attributes` :186
          - `method extract_attribute_name` :222
          - `method qualified` :240
          - `method make_node` :248
          - `method type_name` :275
          - `method collect_names` :294
          - `method visit_items` :326
          - `method visit_item` :334
          - `method visit_function` :349
          - `method collect_uses_edges` :398
          - `method collect_calls` :444
          - `method callee_name` :488
          - `method record_call` :505
          - `method visit_type_item` :527
          - `method visit_trait` :556
          - `method visit_impl` :585
          - `method visit_mod` :630
          - `method visit_const` :652
          - `method visit_type_alias` :668
          - `method visit_macro_def` :689
          - `method collect_imports` :712
          - `method collect_import_leaves` :728
        - `function is_primitive` :767
        - `module tests` :838
          - `function parse` :849
          - `function parses_free_function` :855
          - `function parses_struct` :863
          - `function parses_trait_impl_and_method` :874
          - `function parses_module_with_items` :905
          - `function qualified_name_includes_module_path` :933
          - `function detects_intra_file_calls` :945
          - `function detects_uses_edges_for_param_types` :956
          - `function deferred_calls_capture_unknown_callees` :967

## crates/gitcortex-indexer/src


## crates/gitcortex-indexer/src/differ.rs


## crates/gitcortex-indexer/src/indexer.rs


## crates/gitcortex-indexer/src/lib.rs


## crates/gitcortex-indexer/src/parser


## crates/gitcortex-indexer/src/parser/go.rs


## crates/gitcortex-indexer/src/parser/java.rs


## crates/gitcortex-indexer/src/parser/mod.rs


## crates/gitcortex-indexer/src/parser/python.rs


## crates/gitcortex-indexer/src/parser/rust.rs


## crates/gitcortex-indexer/src/parser/typescript.rs


## crates/gitcortex-mcp


## crates/gitcortex-mcp/src


## crates/gitcortex-mcp/src/cmd


## crates/gitcortex-mcp/src/cmd/blast_radius.rs


## crates/gitcortex-mcp/src/cmd/clean.rs


## crates/gitcortex-mcp/src/cmd/export.rs


## crates/gitcortex-mcp/src/cmd/hook.rs


## crates/gitcortex-mcp/src/cmd/init

- `pub folder init` :1
  - `pub file detect.rs` :1
    - `pub function detect_editors` :7
    - `pub function parse_editor_flag` :34
    - `function env_prefix` :49
  - `pub file mod.rs` :1
    - `module detect` :5
    - `pub module editors` :6
    - `module helpers` :7
    - `module universal` :8
    - `pub function run` :15
  - `pub file universal.rs` :1
    - `constant HOOK_NAMES` :12
    - `constant HOOK_SHEBANG` :19
    - `constant AGENT_GUIDE` :22
    - `pub function install_hooks` :72
    - `pub function initial_index` :99
    - `pub function write_agent_guide` :117
    - `pub function write_ci_workflow` :127
  - `pub file helpers.rs` :1
    - `pub function repo_root` :5
    - `pub function home_dir` :18
    - `pub function current_branch` :25
  - `pub folder editors` :1
    - `pub file copilot.rs` :1
      - `constant COPILOT_INSTRUCTIONS` :5
      - `pub function install` :38
    - `pub file windsurf.rs` :1
      - `constant WINDSURF_RULES` :8
      - `pub function install` :37
      - `function write_windsurf_rules` :43
      - `function write_windsurf_mcp` :58
    - `pub file antigravity.rs` :1
      - `pub function install` :8
      - `function write_antigravity_mcp` :13
    - `pub file cursor.rs` :1
      - `constant CURSOR_RULES` :6
      - `pub function install` :44
      - `function write_cursor_rules` :50
      - `function write_cursor_mcp` :60
    - `pub file mod.rs` :1
      - `pub module antigravity` :5
      - `pub module claude` :6
      - `pub module copilot` :7
      - `pub module cursor` :8
      - `pub module windsurf` :9
      - `pub enum EditorKind` :12
        - `pub method all` :21
        - `pub method display_name` :31
      - `pub function install_for_editor` :42
    - `pub file claude.rs` :1
      - `constant CLAUDE_MD_SECTION` :10
      - `constant PRE_TOOL_USE_HOOK` :22
      - `constant SKILLS` :41
      - `constant SLASH_COMMANDS` :105
      - `pub function install` :128
      - `function write_mcp_json` :138
      - `function write_slash_commands` :158
      - `function write_skills` :172
      - `function update_claude_md` :186
      - `function write_pre_tool_use_hook` :203
      - `function write_claude_settings` :219
      - `function add_gcx_hook_entry` :238

## crates/gitcortex-mcp/src/cmd/init/detect.rs


## crates/gitcortex-mcp/src/cmd/init/editors


## crates/gitcortex-mcp/src/cmd/init/editors/antigravity.rs


## crates/gitcortex-mcp/src/cmd/init/editors/claude.rs


## crates/gitcortex-mcp/src/cmd/init/editors/copilot.rs


## crates/gitcortex-mcp/src/cmd/init/editors/cursor.rs


## crates/gitcortex-mcp/src/cmd/init/editors/mod.rs


## crates/gitcortex-mcp/src/cmd/init/editors/windsurf.rs


## crates/gitcortex-mcp/src/cmd/init/helpers.rs


## crates/gitcortex-mcp/src/cmd/init/mod.rs


## crates/gitcortex-mcp/src/cmd/init/universal.rs


## crates/gitcortex-mcp/src/cmd/mod.rs


## crates/gitcortex-mcp/src/cmd/query.rs


## crates/gitcortex-mcp/src/cmd/serve.rs


## crates/gitcortex-mcp/src/cmd/status.rs


## crates/gitcortex-mcp/src/cmd/viz.rs


## crates/gitcortex-mcp/src/main.rs


## crates/gitcortex-mcp/src/mcp

- `pub folder mcp` :1
  - `pub file mod.rs` :1
    - `pub module server` :1
    - `pub module tools` :2
  - `pub file tools.rs` :1
    - `pub struct LookupSymbolParams` :23
    - `pub struct FindCallersParams` :34
    - `pub struct ContextParams` :44
    - `pub struct ListDefinitionsParams` :52
    - `pub struct BranchDiffParams` :59
    - `pub struct DetectChangesParams` :65
    - `pub struct FindCalleesParams` :71
    - `pub struct FindImplementorsParams` :80
    - `pub struct TracePathParams` :87
    - `pub struct ListSymbolsInRangeParams` :96
    - `pub struct FindUnusedSymbolsParams` :107
    - `pub struct GetSubgraphParams` :114
    - `pub struct GitCortexServer` :129
      - `pub method new` :135
      - `method lookup_symbol` :152
      - `method find_callers` :190
      - `method context` :269
      - `method list_definitions` :310
      - `method branch_diff_graph` :343
      - `method detect_changes` :380
      - `method find_callees` :456
      - `method find_implementors` :490
      - `method trace_path` :520
      - `method list_symbols_in_range` :551
      - `method find_unused_symbols` :584
      - `method get_subgraph` :626
      - `method detect_impact` :690
      - `method generate_map` :726
    - `pub struct DetectImpactParams` :666
    - `pub struct GenerateMapParams` :674
    - `function run_git_diff` :777
    - `function parse_diff_hunks` :791
    - `function parse_hunk_header` :820
  - `pub file server.rs` :1
    - `pub function serve` :8

## crates/gitcortex-mcp/src/mcp/mod.rs


## crates/gitcortex-mcp/src/mcp/server.rs


## crates/gitcortex-mcp/src/mcp/tools.rs


## crates/gitcortex-store


## crates/gitcortex-store/src


## crates/gitcortex-store/src/branch.rs


## crates/gitcortex-store/src/kuzu.rs


## crates/gitcortex-store/src/lib.rs


## crates/gitcortex-store/src/schema.rs


## crates/gitcortex-store/tests

- `pub folder tests` :1
  - `pub file round_trip.rs` :1
    - `function make_node` :10
    - `function tmp_store` :32
    - `function insert_and_lookup_node` :39
    - `function list_definitions_ordered_by_line` :59
    - `function find_callers_via_calls_edge` :83
    - `function delete_file_removes_nodes` :107
    - `function last_indexed_sha_round_trip` :131
    - `function branch_diff_detects_added_and_removed_nodes` :146

## crates/gitcortex-store/tests/round_trip.rs


