# Codebase Map

> Branch: `feature/v0-2-x` · 674 definitions · SHA: `cd1e01e3c4618220df9b17aaa9d08dcdfb5bafa5`

## crates

- `pub folder crates` :1
  - `pub folder gitcortex-indexer` :1
    - `pub folder src` :1
      - `pub folder parser` :1
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
            - `method maybe_visit_constant` :490
            - `method collect_imports` :516
            - `method collect_calls` :572
            - `method callee_name` :589
            - `method record_call` :600
            - `method fn_is_async` :625
            - `method body_has_yield` :633
            - `method collect_decorators` :648
            - `method decorator_name` :657
            - `method extract_param_types` :680
            - `method extract_return_type` :698
            - `method collect_type_names` :707
            - `method walk_type_names` :713
          - `function is_builtin_type` :733
          - `module tests` :792
            - `function parse` :798
            - `function parse_full` :811
            - `function parses_free_function` :835
            - `function parses_class_and_method` :848
            - `function detects_call_edges` :869
            - `function detects_base_class_implements` :877
            - `function detects_type_annotation_uses` :887
            - `function detects_decorator_uses` :898
            - `function detects_import_statement` :908
            - `function detects_from_import_statement` :922
            - `function module_node_is_emitted` :936
            - `function async_function_flagged` :947
            - `function protocol_class_becomes_interface` :961
            - `function non_protocol_class_is_struct` :974
            - `function property_decorator_yields_property_kind` :989
            - `function staticmethod_decorator_sets_is_static` :1002
            - `function classmethod_decorator_sets_is_static` :1015
            - `function dataclass_decorator_class_is_struct` :1028
            - `function generator_function_sets_is_generator` :1042
            - `function async_generator_is_both_async_and_generator` :1055
            - `function nested_yield_does_not_pollute_outer_function` :1068
            - `function module_constant_all_caps_detected` :1086
            - `function nested_class_emits_contains_edge_from_parent` :1105
            - `function multiple_type_annotations_produce_uses_entries` :1123
            - `function private_method_has_private_visibility` :1135
            - `function calls_edge_between_two_functions` :1148
            - `function method_call_via_self_creates_calls_edge` :1156
            - `function aliased_import_uses_alias_name` :1172
            - `function dotted_import_records_leaf_module` :1188
  - `pub folder gitcortex-mcp` :1
    - `pub folder tests` :1
      - `pub file full_pipeline.rs` :1
        - `constant KUZU_LOCK` :12
        - `constant FIXTURES` :18
        - `function init_repo` :20
        - `function commit_file` :35
        - `function run_pipeline` :51
        - `function rust_fixture_indexes_nodes_and_edges` :74
        - `function python_fixture_indexes_nodes_and_edges` :85
        - `function typescript_fixture_indexes_nodes_and_edges` :95
        - `function go_fixture_indexes_nodes_and_edges` :106
        - `function java_fixture_indexes_nodes_and_edges` :117
        - `function run_python_comprehensive` :131
        - `function python_comprehensive_constants_are_indexed` :136
        - `function python_comprehensive_protocols_become_interfaces` :155
        - `function python_comprehensive_plain_classes_are_structs` :177
        - `function python_comprehensive_property_decorator` :190
        - `function python_comprehensive_staticmethod_and_classmethod` :203
        - `function python_comprehensive_async_methods_flagged` :221
        - `function python_comprehensive_generator_function_flagged` :239
        - `function python_comprehensive_async_generator_flagged` :251
        - `function python_comprehensive_nested_classes_indexed` :262
        - `function python_comprehensive_call_edges_recorded` :283
        - `function python_comprehensive_inheritance_edges_present` :300
        - `function python_comprehensive_private_method_visibility` :310
        - `function python_comprehensive_dataclass_is_struct` :323

## crates/gitcortex-core

- `pub folder gitcortex-core` :1
  - `pub folder src` :1
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

- `pub folder src` :1
  - `pub file main.rs` :1
    - `module cmd` :1
    - `module mcp` :2
    - `struct Cli` :11
    - `pub enum VizFormat` :17
    - `enum Commands` :25
    - `enum QueryCmd` :96
    - `function main` :165
  - `pub folder cmd` :1
    - `pub file query.rs` :1
      - `pub function run` :9
      - `function parse_node_kind` :229
      - `function repo_root` :242
    - `pub file doctor.rs` :1
      - `pub function run` :7
      - `function check_hook` :90
      - `type_alias EditorCheck` :112
      - `function check_editor_mcp` :114
      - `function ok` :171
      - `function fail` :175
      - `function warn` :181
      - `function info` :185
      - `function print_summary` :189
      - `function find_repo_root` :197
      - `function current_branch` :210
      - `function head_sha` :226
      - `function dirs_home` :234
  - `pub folder mcp` :1
    - `pub file tools.rs` :1
      - `pub struct LookupSymbolParams` :23
      - `pub struct FindCallersParams` :34
      - `pub struct SymbolContextParams` :44
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
        - `pub method new` :136
        - `method lookup_symbol` :170
        - `method find_callers` :208
        - `method symbol_context` :287
        - `method list_definitions` :328
        - `method branch_diff_graph` :361
        - `method detect_changes` :416
        - `method find_callees` :492
        - `method find_implementors` :536
        - `method trace_path` :574
        - `method list_symbols_in_range` :610
        - `method find_unused_symbols` :651
        - `method get_subgraph` :701
        - `method detect_impact` :777
        - `method generate_map` :813
      - `function detect_current_branch` :147
      - `pub struct DetectImpactParams` :753
      - `pub struct GenerateMapParams` :761
      - `function run_git_diff` :864
      - `function parse_diff_hunks` :878
      - `function parse_hunk_header` :907

## crates/gitcortex-mcp/src/cmd


## crates/gitcortex-mcp/src/cmd/blast_radius.rs


## crates/gitcortex-mcp/src/cmd/clean.rs


## crates/gitcortex-mcp/src/cmd/doctor.rs


## crates/gitcortex-mcp/src/cmd/export.rs


## crates/gitcortex-mcp/src/cmd/hook.rs


## crates/gitcortex-mcp/src/cmd/init

- `pub folder init` :1
  - `pub file helpers.rs` :1
    - `pub function repo_root` :5
    - `pub function home_dir` :18
    - `pub function current_branch` :25
  - `pub file mod.rs` :1
    - `module detect` :5
    - `pub module editors` :6
    - `module helpers` :7
    - `module universal` :8
    - `pub function run` :15
  - `pub file detect.rs` :1
    - `pub function detect_editors` :7
    - `pub function parse_editor_flag` :34
    - `function env_prefix` :49
  - `pub file universal.rs` :1
    - `constant HOOK_NAMES` :12
    - `constant HOOK_SHEBANG` :19
    - `constant AGENT_GUIDE` :22
    - `pub function install_hooks` :72
    - `pub function initial_index` :99
    - `pub function write_agent_guide` :117
    - `pub function write_ci_workflow` :127
  - `pub folder editors` :1
    - `pub file copilot.rs` :1
      - `constant COPILOT_INSTRUCTIONS` :5
      - `pub function install` :38
    - `pub file antigravity.rs` :1
      - `pub function install` :8
      - `function write_antigravity_mcp` :13
    - `pub file windsurf.rs` :1
      - `constant WINDSURF_RULES` :8
      - `pub function install` :37
      - `function write_windsurf_rules` :43
      - `function write_windsurf_mcp` :58
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
    - `pub file cursor.rs` :1
      - `constant CURSOR_RULES` :6
      - `pub function install` :44
      - `function write_cursor_rules` :50
      - `function write_cursor_mcp` :60

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


## crates/gitcortex-mcp/src/cmd/update.rs


## crates/gitcortex-mcp/src/cmd/viz.rs


## crates/gitcortex-mcp/src/main.rs


## crates/gitcortex-mcp/src/mcp


## crates/gitcortex-mcp/src/mcp/mod.rs


## crates/gitcortex-mcp/src/mcp/server.rs


## crates/gitcortex-mcp/src/mcp/tools.rs


## crates/gitcortex-mcp/tests


## crates/gitcortex-mcp/tests/full_pipeline.rs


## crates/gitcortex-store

- `pub folder gitcortex-store` :1
  - `pub folder src` :1
    - `pub file kuzu.rs` :1
      - `pub struct KuzuGraphStore` :23
        - `pub method open` :34
        - `method conn` :59
        - `method ensure_branch` :64
        - `method apply_diff` :75
        - `method lookup_symbol` :333
        - `method find_callers` :354
        - `method find_callers_deep` :372
        - `method symbol_context` :416
        - `method list_definitions` :468
        - `method branch_diff` :484
        - `method list_all_nodes` :524
        - `method list_all_edges` :534
        - `method find_callees` :561
        - `method find_implementors` :608
        - `method trace_path` :625
        - `method list_symbols_in_range` :692
        - `method find_unused_symbols` :717
        - `method get_subgraph` :742
        - `method last_indexed_sha` :866
        - `method set_last_indexed_sha` :870
      - `constant NODE_COLS` :879
      - `function rows_to_nodes` :884
      - `function row_to_node` :895
      - `function collect_ids` :954
      - `function str_val` :968
      - `function i64_val` :980
      - `function bool_val` :990
      - `function kind_from_str` :1004
      - `function edge_kind_from_str` :1025
      - `function vis_str` :1038
      - `function vis_from_str` :1046
      - `function esc` :1058

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
    - `function list_definitions_ordered_by_line` :61
    - `function find_callers_via_calls_edge` :85
    - `function delete_file_removes_nodes` :109
    - `function last_indexed_sha_round_trip` :139
    - `function branch_diff_detects_added_and_removed_nodes` :154

## crates/gitcortex-store/tests/round_trip.rs


## npm

- `pub folder npm` :1
  - `pub folder packages` :1
    - `pub folder gitcortex` :1
      - `pub folder bin` :1
        - `pub file gcx.js` :1
          - `pub module gcx` :1
          - `pub constant PLATFORM_PACKAGES` :7
          - `pub constant BINARY_NAME` :15
          - `pub function findBinary` :17

## npm/packages


## npm/packages/gitcortex


## npm/packages/gitcortex/bin


## npm/packages/gitcortex/bin/gcx.js


## python

- `pub folder python` :1
  - `pub folder src` :1
    - `pub folder gitcortex` :1
      - `pub file __init__.py` :1
        - `pub module __init__` :1
        - `function _binary_path` :9
        - `pub function main` :14

## python/src


## python/src/gitcortex


## python/src/gitcortex/__init__.py


## tests

- `pub folder tests` :1
  - `pub folder integration` :1
    - `pub folder fixtures` :1
      - `pub file python_comprehensive.py` :1
        - `pub constant MAX_RETRIES` :16
        - `pub constant DEFAULT_TIMEOUT` :17
        - `pub constant API_VERSION` :18
        - `pub struct Serializable` :22
          - `pub method serialize` :23
          - `pub method deserialize` :26
        - `pub struct Repository` :30
          - `pub method find_by_id` :31
          - `pub method save` :34
        - `pub struct BaseModel` :40
          - `pub method validate` :41
          - `pub method to_dict` :44
        - `pub struct User` :51
          - `pub method display_name` :57
          - `pub method from_dict` :61
          - `pub method anonymous` :65
          - `pub method validate` :68
          - `method _internal_check` :71
        - `pub struct AsyncService` :77
          - `pub method fetch_user` :78
          - `pub method save_user` :81
        - `pub function user_stream` :87
        - `pub function async_user_stream` :92
        - `pub struct EventSystem` :99
          - `pub method dispatch` :108
        - `pub function create_user` :114
        - `pub function find_users` :118
        - `pub function process_pipeline` :122

## tests/integration


## tests/integration/fixtures


## tests/integration/fixtures/python_comprehensive.py


## tests/integration/fixtures/sample.go


## tests/integration/fixtures/sample.java


## tests/integration/fixtures/sample.py


## tests/integration/fixtures/sample.rs


## tests/integration/fixtures/sample.ts


