# Codebase Map

> Branch: `feature/v0-2` · 357 definitions · SHA: `79632884576bd059c3ba0a823b173d94e1768d17`

## crates

- `pub folder crates` :1
  - `pub folder gitcortex-store` :1
    - `pub folder src` :1
      - `pub file schema.rs` :1
        - `pub function node_table` :9
        - `pub function edge_table` :14
        - `pub function ensure_branch` :22
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
          - `method list_all_nodes` :293
          - `method list_all_edges` :303
          - `method last_indexed_sha` :332
          - `method set_last_indexed_sha` :336
        - `constant NODE_COLS` :345
        - `function rows_to_nodes` :348
        - `function row_to_node` :356
        - `function collect_ids` :396
        - `function str_val` :410
        - `function i64_val` :419
        - `function bool_val` :429
        - `function kind_from_str` :440
        - `function edge_kind_from_str` :457
        - `function vis_str` :467
        - `function vis_from_str` :475
        - `function esc` :487
      - `pub file branch.rs` :1
        - `pub function sanitize` :23
        - `pub function repo_id` :46
        - `pub function data_dir` :55
        - `function home_dir` :62
        - `pub function db_path` :69
        - `pub function last_sha_path` :74
        - `pub function read_last_sha` :80
        - `pub function write_last_sha` :89
        - `module tests` :100
          - `function sanitize_plain` :104
          - `function sanitize_slash_becomes_double_underscore` :109
          - `function sanitize_dash_and_dot` :114
          - `function sanitize_leading_digit` :119
          - `function repo_id_is_stable` :124
          - `function repo_id_differs_across_paths` :130
      - `pub file lib.rs` :1
        - `pub module branch` :1
        - `pub module kuzu` :2
        - `pub module schema` :3
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
  - `pub folder gitcortex-core` :1
    - `pub folder src` :1
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
          - `function graph_diff_is_empty_on_default` :197
      - `pub file store.rs` :1
        - `pub trait GraphStore` :13
      - `pub file error.rs` :1
        - `pub enum GitCortexError` :11
        - `pub type_alias Result` :33
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
      - `pub file lib.rs` :1
        - `pub module error` :1
        - `pub module graph` :2
        - `pub module schema` :3
        - `pub module store` :4
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
      - `pub folder cmd` :1
        - `pub file export.rs` :1
          - `constant DEFAULT_OUTPUT` :15
          - `pub function run` :17
          - `pub function refresh_if_exists` :32
          - `function write_context` :41
          - `function build_context_md` :66
          - `function render_node` :126
          - `function repo_root` :168
          - `function current_branch` :181
        - `pub file hook.rs` :1
          - `pub function run` :11
          - `function repo_root` :75
          - `function current_branch` :85
        - `pub file query.rs` :1
          - `pub function run` :9
          - `function repo_root` :57
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
        - `pub file status.rs` :1
          - `pub function run` :7
          - `function repo_root` :50
          - `function current_branch` :60
        - `pub file clean.rs` :1
          - `pub function run` :8
          - `function repo_root` :26
        - `pub file init.rs` :1
          - `constant HOOK_NAMES` :15
          - `constant HOOK_SHEBANG` :22
          - `constant GH_WORKFLOW` :24
          - `constant CLAUDE_MD_SECTION` :58
          - `constant SKILLS` :72
          - `constant SLASH_COMMANDS` :181
          - `pub function run` :206
          - `function install_hooks` :239
          - `function initial_index` :268
          - `function write_mcp_json` :291
          - `function write_slash_commands` :318
          - `function write_skills` :335
          - `function update_claude_md` :352
          - `function write_ci_workflow` :372
          - `function repo_root` :384
          - `function home_dir` :397
          - `function current_branch` :404
        - `pub file viz.rs` :1
          - `constant VIZ_HTML` :17
          - `struct AppState` :19
          - `pub function run` :24
          - `function serve` :48
          - `function root_handler` :68
          - `function data_handler` :72
          - `function build_dot` :117
          - `function dot_escape` :145
          - `function kind_dot_color` :149
          - `function repo_root` :167
        - `pub file blast_radius.rs` :1
          - `pub enum BlastFormat` :12
          - `pub function run` :19
          - `function risk_label` :104
          - `function print_text` :113
          - `function print_github_comment` :153
          - `function print_json` :213
          - `function node_to_json` :244
          - `function repo_root` :255
        - `pub file serve.rs` :1
          - `pub function run` :3
      - `pub folder mcp` :1
        - `pub file mod.rs` :1
          - `pub module server` :1
          - `pub module tools` :2
        - `pub file server.rs` :1
          - `pub function serve` :8
        - `pub file tools.rs` :1
          - `pub struct LookupSymbolParams` :23
          - `pub struct FindCallersParams` :31
          - `pub struct ListDefinitionsParams` :38
          - `pub struct BranchDiffParams` :45
          - `pub struct GitCortexServer` :55
            - `pub method new` :60
            - `method lookup_symbol` :76
            - `method find_callers` :111
            - `method list_definitions` :142
            - `method branch_diff_graph` :175
            - `method detect_impact` :234
            - `method generate_map` :270
          - `pub struct DetectImpactParams` :210
          - `pub struct GenerateMapParams` :218
  - `pub folder gitcortex-indexer` :1
    - `pub folder src` :1
      - `pub file indexer.rs` :1
        - `type_alias FileIndexResult` :20
        - `pub struct IncrementalIndexer` :32
          - `pub method new` :41
          - `pub method run` :56
          - `method supported_extensions` :148
          - `method index_file` :152
          - `method should_ignore` :213
        - `function resolve_deferred` :227
        - `function build_structural_nodes` :261
        - `function build_ignorer` :384
      - `pub file lib.rs` :1
        - `pub module differ` :1
        - `pub module indexer` :2
        - `pub module parser` :3
      - `pub file differ.rs` :1
        - `pub enum FileChange` :10
          - `pub method path` :17
        - `pub struct Differ` :27
          - `pub method open` :33
          - `pub method head_sha` :40
          - `pub method changed_files` :57
      - `pub folder parser` :1
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
          - `struct FileVisitor` :113
            - `method new` :126
            - `method text` :138
            - `method span` :142
            - `method visibility` :149
            - `method is_async` :165
            - `method qualified` :171
            - `method make_node` :179
            - `method collect_names` :206
            - `method collect_names_from_var_decl` :238
            - `method unwrap_export` :261
            - `method visit_program` :284
            - `method visit_statement` :293
            - `method visit_function` :322
            - `method visit_class` :354
            - `method visit_method` :385
            - `method visit_interface` :408
            - `method visit_type_alias` :422
            - `method visit_enum` :436
            - `method visit_var_decl` :446
            - `method collect_calls` :498
            - `method callee_name` :516
            - `method record_call` :527
          - `module tests` :553
            - `function parse_ts` :559
            - `function parse_js` :571
            - `function parses_ts_function` :584
            - `function parses_ts_class_and_method` :592
            - `function parses_ts_interface` :613
            - `function parses_js_arrow_function` :621
            - `function detects_ts_call_edges` :632
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
            - `method default` :28
            - `method extensions` :34
            - `method parse` :38
          - `struct FileVisitor` :71
            - `method new` :84
            - `method text` :96
            - `method span` :100
            - `method visibility` :108
            - `method qualified` :116
            - `method make_node` :124
            - `method collect_names` :153
            - `method visit_module` :184
            - `method visit_top_level` :192
            - `method visit_function` :217
            - `method visit_class` :254
            - `method maybe_visit_constant` :298
            - `method collect_calls` :323
            - `method callee_name` :341
            - `method record_call` :352
          - `module tests` :378
            - `function parse` :384
            - `function parses_free_function` :397
            - `function parses_class_and_method` :405
            - `function detects_call_edges` :426
        - `pub file go.rs` :1
          - `pub struct GoParser` :15
            - `pub method new` :20
            - `method default` :28
            - `method extensions` :34
            - `method parse` :38
          - `struct FileVisitor` :71
            - `method new` :84
            - `method text` :96
            - `method span` :100
            - `method visibility` :108
            - `method qualified` :121
            - `method make_node` :129
            - `method collect_names` :156
            - `method collect_type_names` :181
            - `method visit_source_file` :200
            - `method visit_top_level` :208
            - `method visit_function` :218
            - `method visit_method` :236
            - `method receiver_type` :269
            - `method visit_type_decl` :296
            - `method visit_const_decl` :333
            - `method collect_calls` :349
            - `method callee_name` :366
            - `method record_call` :377
          - `module tests` :403
            - `function parse` :409
            - `function parses_function` :420
            - `function parses_struct_and_method` :432
            - `function parses_interface` :453
            - `function go_visibility_is_uppercase` :462
            - `function detects_call_edges` :473
        - `pub file rust.rs` :1
          - `pub struct RustParser` :17
            - `pub method new` :22
            - `method default` :30
            - `method extensions` :36
            - `method parse` :40
          - `struct FileVisitor` :78
            - `method new` :92
            - `method text` :109
            - `method field_text` :113
            - `method span` :119
            - `method visibility` :126
            - `method is_async` :141
            - `method is_unsafe` :147
            - `method qualified` :153
            - `method make_node` :161
            - `method type_name` :186
            - `method collect_names` :205
            - `method visit_items` :237
            - `method visit_item` :245
            - `method visit_function` :260
            - `method collect_uses_edges` :304
            - `method collect_calls` :350
            - `method callee_name` :394
            - `method record_call` :411
            - `method visit_type_item` :433
            - `method visit_trait` :459
            - `method visit_impl` :485
            - `method visit_mod` :530
            - `method visit_const` :552
            - `method visit_type_alias` :568
            - `method visit_macro_def` :589
            - `method collect_imports` :612
            - `method collect_import_leaves` :628
          - `function is_primitive` :667
          - `module tests` :738
            - `function parse` :749
            - `function parses_free_function` :755
            - `function parses_struct` :763
            - `function parses_trait_impl_and_method` :774
            - `function parses_module_with_items` :805
            - `function qualified_name_includes_module_path` :833
            - `function detects_intra_file_calls` :845
            - `function detects_uses_edges_for_param_types` :856
            - `function deferred_calls_capture_unknown_callees` :867

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


## crates/gitcortex-store/src


## crates/gitcortex-store/src/branch.rs


## crates/gitcortex-store/src/kuzu.rs


## crates/gitcortex-store/src/lib.rs


## crates/gitcortex-store/src/schema.rs


## crates/gitcortex-store/tests


## crates/gitcortex-store/tests/round_trip.rs


