# Codebase Map

> Branch: `feature/v0-2` · 553 definitions · SHA: `d752feaf2281ac8eaef55df5f670d8710e6eb2ed`

## crates

- `pub folder crates` :1
  - `pub folder gitcortex-indexer` :1
    - `pub folder src` :1
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
  - `pub folder gitcortex-mcp` :1
    - `pub folder src` :1
      - `pub file main.rs` :1
        - `module cmd` :1
        - `module mcp` :2
        - `struct Cli` :11
        - `pub enum VizFormat` :17
        - `enum Commands` :25
        - `enum QueryCmd` :92
        - `function main` :120

## crates/gitcortex-core

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
    - `pub file store.rs` :1
      - `pub struct SubGraph` :10
      - `pub struct CallersDeep` :16
      - `pub struct SymbolContext` :24
      - `pub trait GraphStore` :40

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


## crates/gitcortex-indexer/src/parser/java.rs


## crates/gitcortex-indexer/src/parser/mod.rs


## crates/gitcortex-indexer/src/parser/python.rs


## crates/gitcortex-indexer/src/parser/rust.rs


## crates/gitcortex-indexer/src/parser/typescript.rs


## crates/gitcortex-mcp


## crates/gitcortex-mcp/src


## crates/gitcortex-mcp/src/cmd

- `pub folder cmd` :1
  - `pub folder init` :1
    - `pub file detect.rs` :1
      - `pub function detect_editors` :7
      - `pub function parse_editor_flag` :34
      - `function env_prefix` :49
    - `pub file helpers.rs` :1
      - `pub function repo_root` :5
      - `pub function home_dir` :18
      - `pub function current_branch` :25
    - `pub file universal.rs` :1
      - `constant HOOK_NAMES` :12
      - `constant HOOK_SHEBANG` :19
      - `constant AGENT_GUIDE` :22
      - `pub function install_hooks` :72
      - `pub function initial_index` :99
      - `pub function write_agent_guide` :117
      - `pub function write_ci_workflow` :127
    - `pub file mod.rs` :1
      - `module detect` :5
      - `pub module editors` :6
      - `module helpers` :7
      - `module universal` :8
      - `pub function run` :15
    - `pub folder editors` :1
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

## crates/gitcortex-mcp/src/cmd/blast_radius.rs


## crates/gitcortex-mcp/src/cmd/clean.rs


## crates/gitcortex-mcp/src/cmd/export.rs


## crates/gitcortex-mcp/src/cmd/hook.rs


## crates/gitcortex-mcp/src/cmd/init


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

## crates/gitcortex-mcp/src/mcp/mod.rs


## crates/gitcortex-mcp/src/mcp/server.rs


## crates/gitcortex-mcp/src/mcp/tools.rs


## crates/gitcortex-store

- `pub folder gitcortex-store` :1
  - `pub folder src` :1
    - `pub file schema.rs` :1
      - `pub function node_table` :9
      - `pub function edge_table` :14
      - `pub function ensure_branch` :22
    - `pub file kuzu.rs` :1
      - `pub struct KuzuGraphStore` :23
        - `pub method open` :34
        - `method conn` :59
        - `method ensure_branch` :64
        - `method apply_diff` :75
        - `method lookup_symbol` :262
        - `method find_callers` :283
        - `method find_callers_deep` :301
        - `method symbol_context` :345
        - `method list_definitions` :397
        - `method branch_diff` :413
        - `method list_all_nodes` :453
        - `method list_all_edges` :463
        - `method find_callees` :490
        - `method find_implementors` :537
        - `method trace_path` :579
        - `method list_symbols_in_range` :645
        - `method find_unused_symbols` :670
        - `method get_subgraph` :695
        - `method last_indexed_sha` :810
        - `method set_last_indexed_sha` :814
      - `constant NODE_COLS` :823
      - `function rows_to_nodes` :828
      - `function row_to_node` :836
      - `function collect_ids` :895
      - `function str_val` :909
      - `function i64_val` :918
      - `function bool_val` :928
      - `function kind_from_str` :939
      - `function edge_kind_from_str` :960
      - `function vis_str` :973
      - `function vis_from_str` :981
      - `function esc` :993
    - `pub file branch.rs` :1
      - `pub function sanitize` :23
      - `pub function repo_id` :46
      - `pub function data_dir` :55
      - `function home_dir` :62
      - `pub function db_path` :69
      - `pub function last_sha_path` :74
      - `pub function schema_version_path` :79
      - `pub function read_schema_version` :84
      - `pub function write_schema_version` :93
      - `pub function wipe_repo_data` :102
      - `pub function read_last_sha` :109
      - `pub function write_last_sha` :118
      - `module tests` :129
        - `function sanitize_plain` :133
        - `function sanitize_slash_becomes_double_underscore` :138
        - `function sanitize_dash_and_dot` :143
        - `function sanitize_leading_digit` :148
        - `function repo_id_is_stable` :153
        - `function repo_id_differs_across_paths` :159

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


