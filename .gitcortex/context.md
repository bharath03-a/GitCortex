# Codebase Map

> Branch: `main` · 356 definitions · SHA: `56772165c8a77018fe9206a11ecf4bd323fd198d`

## crates

- `pub folder crates` :1
  - `pub folder gitcortex-indexer` :1
    - `pub folder src` :1
      - `pub file indexer.rs` :1
        - `pub struct IncrementalIndexer` :24
          - `pub method new` :33
          - `pub method run` :45
          - `method supported_extensions` :117
          - `method index_file` :121
          - `method should_ignore` :160
        - `function resolve_deferred` :172
        - `function build_structural_nodes` :201
        - `function build_ignorer` :292
      - `pub folder parser` :1
        - `pub file rust.rs` :1
          - `pub struct RustParser` :17
            - `pub method new` :22
            - `method default` :28
            - `method extensions` :32
            - `method parse` :34
          - `struct FileVisitor` :68
            - `method new` :82
            - `method text` :99
            - `method field_text` :103
            - `method span` :109
            - `method visibility` :116
            - `method is_async` :127
            - `method is_unsafe` :133
            - `method qualified` :139
            - `method make_node` :147
            - `method type_name` :165
            - `method collect_names` :178
            - `method visit_items` :210
            - `method visit_item` :218
            - `method visit_function` :233
            - `method collect_uses_edges` :268
            - `method collect_calls` :306
            - `method callee_name` :350
            - `method record_call` :367
            - `method visit_type_item` :379
            - `method visit_trait` :395
            - `method visit_impl` :411
            - `method visit_mod` :449
            - `method visit_const` :465
            - `method visit_type_alias` :475
            - `method visit_macro_def` :485
            - `method collect_imports` :497
            - `method collect_import_leaves` :513
          - `function is_primitive` :547
          - `module tests` :570
            - `function parse` :581
            - `function parses_free_function` :587
            - `function parses_struct` :595
            - `function parses_trait_impl_and_method` :603
            - `function parses_module_with_items` :625
            - `function qualified_name_includes_module_path` :644
            - `function detects_intra_file_calls` :656
            - `function detects_uses_edges_for_param_types` :667
            - `function deferred_calls_capture_unknown_callees` :678
        - `pub file mod.rs` :1
          - `pub module go` :8
          - `pub module python` :9
          - `pub module rust` :10
          - `pub module typescript` :11
          - `pub struct ParseResult` :14
          - `pub trait LanguageParser` :33
          - `pub function parser_for_path` :44
        - `pub file python.rs` :1
          - `pub struct PythonParser` :15
            - `pub method new` :20
            - `method default` :26
            - `method extensions` :30
            - `method parse` :32
          - `struct FileVisitor` :61
            - `method new` :74
            - `method text` :86
            - `method span` :90
            - `method visibility` :98
            - `method qualified` :102
            - `method make_node` :110
            - `method collect_names` :139
            - `method visit_module` :170
            - `method visit_top_level` :178
            - `method visit_function` :203
            - `method visit_class` :226
            - `method maybe_visit_constant` :257
            - `method collect_calls` :280
            - `method callee_name` :298
            - `method record_call` :309
          - `module tests` :325
            - `function parse` :331
            - `function parses_free_function` :337
            - `function parses_class_and_method` :345
            - `function detects_call_edges` :357
        - `pub file typescript.rs` :1
          - `pub struct TypeScriptParser` :21
            - `pub method new_ts` :26
            - `pub method new_tsx` :30
            - `method extensions` :50
            - `method parse` :52
          - `pub struct JavaScriptParser` :35
            - `pub method new` :40
            - `method default` :46
            - `method extensions` :58
            - `method parse` :60
          - `function parse_source` :65
          - `struct FileVisitor` :93
            - `method new` :106
            - `method text` :118
            - `method span` :122
            - `method visibility` :129
            - `method is_async` :145
            - `method qualified` :151
            - `method make_node` :159
            - `method collect_names` :179
            - `method collect_names_from_var_decl` :211
            - `method unwrap_export` :225
            - `method visit_program` :244
            - `method visit_statement` :253
            - `method visit_function` :277
            - `method visit_class` :299
            - `method visit_method` :324
            - `method visit_interface` :337
            - `method visit_type_alias` :345
            - `method visit_enum` :353
            - `method visit_var_decl` :361
            - `method collect_calls` :398
            - `method callee_name` :416
            - `method record_call` :427
          - `module tests` :443
            - `function parse_ts` :449
            - `function parse_js` :454
            - `function parses_ts_function` :460
            - `function parses_ts_class_and_method` :468
            - `function parses_ts_interface` :480
            - `function parses_js_arrow_function` :488
            - `function detects_ts_call_edges` :496
        - `pub file go.rs` :1
          - `pub struct GoParser` :15
            - `pub method new` :20
            - `method default` :26
            - `method extensions` :30
            - `method parse` :32
          - `struct FileVisitor` :61
            - `method new` :74
            - `method text` :86
            - `method span` :90
            - `method visibility` :98
            - `method qualified` :106
            - `method make_node` :114
            - `method collect_names` :134
            - `method collect_type_names` :159
            - `method visit_source_file` :176
            - `method visit_top_level` :184
            - `method visit_function` :194
            - `method visit_method` :206
            - `method receiver_type` :229
            - `method visit_type_decl` :253
            - `method visit_const_decl` :280
            - `method collect_calls` :292
            - `method callee_name` :309
            - `method record_call` :320
          - `module tests` :336
            - `function parse` :342
            - `function parses_function` :348
            - `function parses_struct_and_method` :357
            - `function parses_interface` :369
            - `function go_visibility_is_uppercase` :378
            - `function detects_call_edges` :389
  - `pub folder gitcortex-core` :1
    - `pub folder src` :1
      - `pub file schema.rs` :1
        - `pub enum NodeKind` :6
          - `method fmt` :21
        - `pub enum EdgeKind` :42
          - `method fmt` :56
        - `pub enum Visibility` :71
          - `method fmt` :79
        - `pub enum SolidHint` :93
        - `pub enum DesignPattern` :109
        - `pub enum CodeSmell` :122
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
        - `pub struct Node` :86
        - `pub struct Edge` :101
        - `pub struct GraphDiff` :112
          - `pub method is_empty` :134
          - `pub method merge` :148
        - `module tests` :163
          - `function node_id_is_unique` :167
          - `function graph_diff_merge` :174
          - `function graph_diff_is_empty_on_default` :191
  - `pub folder gitcortex-mcp` :1
    - `pub folder src` :1
      - `pub file main.rs` :1
        - `module cmd` :1
        - `module mcp` :2
        - `struct Cli` :11
        - `pub enum VizFormat` :17
        - `enum Commands` :25
        - `enum QueryCmd` :87
        - `function main` :108
      - `pub folder mcp` :1
        - `pub file tools.rs` :1
          - `pub struct LookupSymbolParams` :23
          - `pub struct FindCallersParams` :31
          - `pub struct ListDefinitionsParams` :38
          - `pub struct BranchDiffParams` :45
          - `pub struct GitCortexServer` :55
            - `pub method new` :60
            - `method lookup_symbol` :74
            - `method find_callers` :110
            - `method list_definitions` :142
            - `method branch_diff_graph` :176
            - `method detect_impact` :240
            - `method generate_map` :273
          - `pub struct DetectImpactParams` :218
          - `pub struct GenerateMapParams` :226
      - `pub folder cmd` :1
        - `pub file status.rs` :1
          - `pub function run` :7
          - `function repo_root` :48
          - `function current_branch` :58
        - `pub file mod.rs` :1
          - `pub module blast_radius` :1
          - `pub module clean` :2
          - `pub module export` :3
          - `pub module hook` :4
          - `pub module init` :5
          - `pub module query` :6
          - `pub module serve` :7
          - `pub module status` :8
          - `pub module viz` :9
        - `pub file clean.rs` :1
          - `pub function run` :8
          - `function repo_root` :26
        - `pub file viz.rs` :1
          - `constant VIZ_HTML` :17
          - `struct AppState` :19
          - `pub function run` :24
          - `function serve` :48
          - `function root_handler` :68
          - `function data_handler` :72
          - `function build_dot` :107
          - `function dot_escape` :133
          - `function kind_dot_color` :137
          - `function repo_root` :155
        - `pub file blast_radius.rs` :1
          - `pub enum BlastFormat` :12
          - `pub function run` :19
          - `function risk_label` :103
          - `function print_text` :112
          - `function print_github_comment` :158
          - `function print_json` :220
          - `function node_to_json` :251
          - `function repo_root` :262
        - `pub file export.rs` :1
          - `constant DEFAULT_OUTPUT` :15
          - `pub function run` :17
          - `pub function refresh_if_exists` :32
          - `function write_context` :41
          - `function build_context_md` :66
          - `function render_node` :127
          - `function repo_root` :165
          - `function current_branch` :178

## crates/gitcortex-core


## crates/gitcortex-core/src


## crates/gitcortex-core/src/error.rs


## crates/gitcortex-core/src/graph.rs


## crates/gitcortex-core/src/lib.rs


## crates/gitcortex-core/src/schema.rs


## crates/gitcortex-core/src/store.rs


## crates/gitcortex-indexer


## crates/gitcortex-indexer/src


## crates/gitcortex-indexer/src/differ.rs


## crates/gitcortex-indexer/src/indexer.rs


## crates/gitcortex-indexer/src/lib.rs


## crates/gitcortex-indexer/src/parser


## crates/gitcortex-indexer/src/parser/go.rs


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


## crates/gitcortex-mcp/src/cmd/init.rs


## crates/gitcortex-mcp/src/cmd/mod.rs


## crates/gitcortex-mcp/src/cmd/query.rs


## crates/gitcortex-mcp/src/cmd/serve.rs


## crates/gitcortex-mcp/src/cmd/status.rs


## crates/gitcortex-mcp/src/cmd/viz.rs


## crates/gitcortex-mcp/src/main.rs


## crates/gitcortex-mcp/src/mcp


## crates/gitcortex-mcp/src/mcp/mod.rs


## crates/gitcortex-mcp/src/mcp/server.rs


## crates/gitcortex-mcp/src/mcp/tools.rs


## crates/gitcortex-store

- `pub folder gitcortex-store` :1
  - `pub folder src` :1
    - `pub file kuzu.rs` :1
      - `pub struct KuzuGraphStore` :20
        - `pub method open` :27
        - `method conn` :43
        - `method ensure_branch` :48
        - `method apply_diff` :59
        - `method lookup_symbol` :201
        - `method find_callers` :217
        - `method list_definitions` :237
        - `method branch_diff` :253
        - `method list_all_nodes` :294
        - `method list_all_edges` :304
        - `method last_indexed_sha` :333
        - `method set_last_indexed_sha` :337
      - `constant NODE_COLS` :346
      - `function rows_to_nodes` :350
      - `function row_to_node` :358
      - `function collect_ids` :389
      - `function str_val` :403
      - `function i64_val` :410
      - `function bool_val` :418
      - `function kind_from_str` :427
      - `function edge_kind_from_str` :444
      - `function vis_str` :454
      - `function vis_from_str` :462
      - `function esc` :474

## crates/gitcortex-store/src


## crates/gitcortex-store/src/branch.rs


## crates/gitcortex-store/src/kuzu.rs


## crates/gitcortex-store/src/lib.rs


## crates/gitcortex-store/src/schema.rs


## crates/gitcortex-store/tests

- `pub folder tests` :1
  - `pub file round_trip.rs` :1
    - `function make_node` :10
    - `function tmp_store` :29
    - `function insert_and_lookup_node` :36
    - `function list_definitions_ordered_by_line` :53
    - `function find_callers_via_calls_edge` :74
    - `function delete_file_removes_nodes` :94
    - `function last_indexed_sha_round_trip` :113
    - `function branch_diff_detects_added_and_removed_nodes` :123

## crates/gitcortex-store/tests/round_trip.rs


