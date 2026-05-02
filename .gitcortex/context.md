# Codebase Map

> Branch: `main` · 357 definitions · SHA: `dd64ae735e086a2e3be565b7769418815df1ee76`

## crates

- `pub folder crates` :1
  - `pub folder gitcortex-mcp` :1
    - `pub folder src` :1
      - `pub folder cmd` :1
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

## crates/gitcortex-core

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
        - `function graph_diff_is_empty_on_default` :197

## crates/gitcortex-core/src


## crates/gitcortex-core/src/error.rs


## crates/gitcortex-core/src/graph.rs


## crates/gitcortex-core/src/lib.rs


## crates/gitcortex-core/src/schema.rs


## crates/gitcortex-core/src/store.rs


## crates/gitcortex-indexer

- `pub folder gitcortex-indexer` :1
  - `pub folder src` :1
    - `pub folder parser` :1
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

- `pub folder mcp` :1
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
  - `pub file server.rs` :1
    - `pub function serve` :8

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

## crates/gitcortex-store/src


## crates/gitcortex-store/src/branch.rs


## crates/gitcortex-store/src/kuzu.rs


## crates/gitcortex-store/src/lib.rs


## crates/gitcortex-store/src/schema.rs


## crates/gitcortex-store/tests


## crates/gitcortex-store/tests/round_trip.rs


