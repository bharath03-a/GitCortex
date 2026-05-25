# Codebase Map

> Branch: `feature/v0-2-x` · 810 definitions · SHA: `5fa1cf1d2b818cbde1f07d95026a7b4c2c6f99b4`

## crates

- `pub folder crates` :1
  - `pub folder gitcortex-mcp` :1
    - `pub folder dist-viz` :1
      - `pub folder assets` :1
    - `pub folder src` :1
      - `pub file lib.rs` :1
        - `pub module mcp` :1
  - `pub folder gitcortex-cli` :1
    - `pub folder src` :1
      - `pub file main.rs` :1
        - `module cmd` :1
        - `struct Cli` :11
        - `enum Commands` :17
        - `pub enum QueryCmd` :88
        - `function main` :157
      - `pub folder cmd` :1
        - `pub file doctor.rs` :1
          - `pub function run` :7
          - `function check_hook` :102
          - `type_alias EditorCheck` :120
          - `function check_editor_mcp` :122
          - `function ok` :198
          - `function fail` :202
          - `function warn` :208
          - `function info` :212
          - `function print_summary` :216
          - `function find_repo_root` :224
          - `function current_branch` :237
          - `function head_sha` :253
          - `function dirs_home` :261
        - `pub file update.rs` :1
          - `constant CURRENT` :3
          - `constant RELEASES_API` :4
          - `pub function run` :6
          - `function fetch_latest_version` :30
          - `enum InstallMethod` :67
            - `method fmt` :75
          - `function detect_install_method` :85
          - `function update_command` :103
        - `pub file blast_radius.rs` :1
          - `pub enum BlastFormat` :12
          - `pub function run` :19
          - `function risk_label` :104
          - `function print_text` :113
          - `function print_github_comment` :153
          - `function print_json` :213
          - `function node_to_json` :244
          - `function repo_root` :255
        - `pub file mod.rs` :1
          - `pub module blast_radius` :1
          - `pub module clean` :2
          - `pub module doctor` :3
          - `pub module export` :4
          - `pub module hook` :5
          - `pub module init` :6
          - `pub module query` :7
          - `pub module serve` :8
          - `pub module status` :9
          - `pub module update` :10
        - `pub file export.rs` :1
          - `constant DEFAULT_OUTPUT` :15
          - `pub function run` :17
          - `pub function refresh_if_exists` :32
          - `function write_context` :41
          - `function build_context_md` :66
          - `function render_node` :126
          - `function repo_root` :168
          - `function current_branch` :181
        - `pub file serve.rs` :1
          - `pub function run` :3
        - `pub file query.rs` :1
          - `pub function run` :9
          - `function parse_node_kind` :242
          - `function repo_root` :255
        - `pub file status.rs` :1
          - `pub function run` :7
          - `function repo_root` :50
          - `function current_branch` :60
        - `pub file clean.rs` :1
          - `pub function run` :8
          - `function repo_root` :26
        - `pub file hook.rs` :1
          - `pub function run` :11
          - `function repo_root` :77
          - `function current_branch` :87
        - `pub folder init` :1
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
          - `pub folder editors` :1
            - `pub file cursor.rs` :1
              - `constant CURSOR_RULES` :6
              - `pub function install` :44
              - `function write_cursor_rules` :50
              - `function write_cursor_mcp` :60
            - `pub file windsurf.rs` :1
              - `constant WINDSURF_RULES` :8
              - `pub function install` :37
              - `function write_windsurf_rules` :43
              - `function write_windsurf_mcp` :58
            - `pub file antigravity.rs` :1
              - `pub function install` :8
              - `function write_antigravity_mcp` :13
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
  - `pub folder gitcortex-viz` :1
    - `pub file build.rs` :1
      - `function main` :12
      - `function ensure_placeholder` :63
      - `function which` :86
    - `pub folder src` :1
      - `pub file lib.rs` :1
        - `constant VIZ_INDEX` :17
        - `constant VIZ_JS` :18
        - `constant VIZ_CSS` :19
        - `constant VIZ_WEBGL` :20
        - `pub enum VizFormat` :24
        - `struct AppState` :35
        - `pub function run` :40
        - `function serve` :65
        - `function root_handler` :96
        - `function js_handler` :100
        - `function css_handler` :104
        - `function webgl_handler` :108
        - `function static_response` :112
        - `function run_blocking` :118
        - `function with_locked_store` :136
        - `function node_json` :147
        - `function data_handler` :166
        - `struct DepthQuery` :200
        - `function symbol_context_handler` :208
        - `function callers_handler` :238
        - `function branches_handler` :280
        - `struct UnusedQuery` :302
        - `function unused_handler` :310
        - `struct BranchDiffQuery` :334
        - `function branch_diff_handler` :340
        - `function parse_node_kind` :365
        - `function list_local_branches_async` :386
        - `function build_dot` :398
        - `function dot_escape` :426
        - `function kind_dot_color` :430
        - `function repo_root` :450

## crates/gitcortex-cli


## crates/gitcortex-cli/src


## crates/gitcortex-cli/src/cmd


## crates/gitcortex-cli/src/cmd/blast_radius.rs


## crates/gitcortex-cli/src/cmd/clean.rs


## crates/gitcortex-cli/src/cmd/doctor.rs


## crates/gitcortex-cli/src/cmd/export.rs


## crates/gitcortex-cli/src/cmd/hook.rs


## crates/gitcortex-cli/src/cmd/init


## crates/gitcortex-cli/src/cmd/init/detect.rs


## crates/gitcortex-cli/src/cmd/init/editors


## crates/gitcortex-cli/src/cmd/init/editors/antigravity.rs


## crates/gitcortex-cli/src/cmd/init/editors/claude.rs


## crates/gitcortex-cli/src/cmd/init/editors/copilot.rs


## crates/gitcortex-cli/src/cmd/init/editors/cursor.rs


## crates/gitcortex-cli/src/cmd/init/editors/mod.rs


## crates/gitcortex-cli/src/cmd/init/editors/windsurf.rs


## crates/gitcortex-cli/src/cmd/init/helpers.rs


## crates/gitcortex-cli/src/cmd/init/mod.rs


## crates/gitcortex-cli/src/cmd/init/universal.rs


## crates/gitcortex-cli/src/cmd/mod.rs


## crates/gitcortex-cli/src/cmd/query.rs


## crates/gitcortex-cli/src/cmd/serve.rs


## crates/gitcortex-cli/src/cmd/status.rs


## crates/gitcortex-cli/src/cmd/update.rs


## crates/gitcortex-cli/src/main.rs


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

- `pub folder gitcortex-indexer` :1
  - `pub folder src` :1
    - `pub file indexer.rs` :1
      - `type_alias FileIndexResult` :20
      - `pub struct IncrementalIndexer` :35
        - `pub method new` :44
        - `pub method run` :59
        - `method supported_extensions` :175
        - `method index_file` :181
        - `method should_ignore` :251
      - `function resolve_deferred` :265
      - `function build_structural_nodes` :299
      - `function build_ignorer` :422
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
          - `function non_protocol_class_is_struct` :977
          - `function property_decorator_yields_property_kind` :992
          - `function staticmethod_decorator_sets_is_static` :1006
          - `function classmethod_decorator_sets_is_static` :1022
          - `function dataclass_decorator_class_is_struct` :1038
          - `function generator_function_sets_is_generator` :1052
          - `function async_generator_is_both_async_and_generator` :1068
          - `function nested_yield_does_not_pollute_outer_function` :1087
          - `function module_constant_all_caps_detected` :1108
          - `function nested_class_emits_contains_edge_from_parent` :1130
          - `function multiple_type_annotations_produce_uses_entries` :1156
          - `function private_method_has_private_visibility` :1169
          - `function calls_edge_between_two_functions` :1188
          - `function method_call_via_self_creates_calls_edge` :1196
          - `function aliased_import_uses_alias_name` :1218
          - `function dotted_import_records_leaf_module` :1234

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


## crates/gitcortex-mcp/dist-viz


## crates/gitcortex-mcp/dist-viz/assets


## crates/gitcortex-mcp/src


## crates/gitcortex-mcp/src/cmd

- `pub folder cmd` :1

## crates/gitcortex-mcp/src/cmd/init

- `pub folder init` :1
  - `pub folder editors` :1

## crates/gitcortex-mcp/src/cmd/init/editors


## crates/gitcortex-mcp/src/lib.rs


## crates/gitcortex-mcp/src/mcp

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
      - `method lookup_symbol` :174
      - `method find_callers` :216
      - `method symbol_context` :299
      - `method list_definitions` :344
      - `method branch_diff_graph` :381
      - `method detect_changes` :436
      - `method find_callees` :516
      - `method find_implementors` :564
      - `method trace_path` :606
      - `method list_symbols_in_range` :646
      - `method find_unused_symbols` :691
      - `method get_subgraph` :745
      - `method detect_impact` :825
      - `method generate_map` :861
    - `function detect_current_branch` :147
    - `pub struct DetectImpactParams` :801
    - `pub struct GenerateMapParams` :809
    - `function run_git_diff` :912
    - `function parse_diff_hunks` :926
    - `function parse_hunk_header` :955

## crates/gitcortex-mcp/src/mcp/mod.rs


## crates/gitcortex-mcp/src/mcp/server.rs


## crates/gitcortex-mcp/src/mcp/tools.rs


## crates/gitcortex-mcp/tests

- `pub folder tests` :1
  - `pub file full_pipeline.rs` :1
    - `constant KUZU_LOCK` :12
    - `constant FIXTURES` :18
    - `function init_repo` :23
    - `function commit_file` :38
    - `function run_pipeline` :51
    - `function rust_fixture_indexes_nodes_and_edges` :81
    - `function python_fixture_indexes_nodes_and_edges` :95
    - `function typescript_fixture_indexes_nodes_and_edges` :108
    - `function go_fixture_indexes_nodes_and_edges` :119
    - `function java_fixture_indexes_nodes_and_edges` :130
    - `function run_python_comprehensive` :144
    - `function python_comprehensive_constants_are_indexed` :152
    - `function python_comprehensive_protocols_become_interfaces` :174
    - `function python_comprehensive_plain_classes_are_structs` :199
    - `function python_comprehensive_property_decorator` :215
    - `function python_comprehensive_staticmethod_and_classmethod` :234
    - `function python_comprehensive_async_methods_flagged` :255
    - `function python_comprehensive_generator_function_flagged` :276
    - `function python_comprehensive_async_generator_flagged` :291
    - `function python_comprehensive_nested_classes_indexed` :314
    - `function python_comprehensive_call_edges_recorded` :345
    - `function python_comprehensive_inheritance_edges_present` :366
    - `function python_comprehensive_private_method_visibility` :379
    - `function python_comprehensive_dataclass_is_struct` :392

## crates/gitcortex-mcp/tests/full_pipeline.rs


## crates/gitcortex-mcp/viz

- `pub folder viz` :1
  - `pub folder src` :1
    - `pub folder hooks` :1
    - `pub folder graph` :1
      - `pub folder __tests__` :1
    - `pub folder components` :1
    - `pub folder theme` :1

## crates/gitcortex-mcp/viz/src


## crates/gitcortex-mcp/viz/src/components


## crates/gitcortex-mcp/viz/src/graph


## crates/gitcortex-mcp/viz/src/graph/__tests__


## crates/gitcortex-mcp/viz/src/hooks


## crates/gitcortex-mcp/viz/src/theme


## crates/gitcortex-store

- `pub folder gitcortex-store` :1
  - `pub folder src` :1
    - `pub file lib.rs` :1
      - `pub module branch` :1
      - `pub module kuzu` :4
      - `pub module schema` :6
      - `pub module memory` :9
    - `pub file memory.rs` :1
      - `pub struct MemoryGraphStore` :15
        - `pub method open` :18
        - `method apply_diff` :24
        - `method lookup_symbol` :28
        - `method find_callers` :32
        - `method find_callers_deep` :36
        - `method symbol_context` :45
        - `method list_definitions` :51
        - `method list_all_nodes` :55
        - `method list_all_edges` :59
        - `method branch_diff` :63
        - `method find_callees` :67
        - `method find_implementors` :76
        - `method trace_path` :84
        - `method list_symbols_in_range` :88
        - `method find_unused_symbols` :98
        - `method get_subgraph` :102
        - `method last_indexed_sha` :112
        - `method set_last_indexed_sha` :116
  - `pub folder tests` :1
    - `pub file round_trip.rs` :1
      - `function make_node` :12
      - `function tmp_store` :34
      - `function insert_and_lookup_node` :41
      - `function list_definitions_ordered_by_line` :63
      - `function find_callers_via_calls_edge` :87
      - `function delete_file_removes_nodes` :111
      - `function last_indexed_sha_round_trip` :141
      - `function branch_diff_detects_added_and_removed_nodes` :156

## crates/gitcortex-store/src


## crates/gitcortex-store/src/branch.rs


## crates/gitcortex-store/src/kuzu

- `pub folder kuzu` :1
  - `pub file mod.rs` :1
    - `module conv` :16
    - `module escape` :17
    - `module queries` :18
    - `module values` :19
    - `pub struct KuzuGraphStore` :33
      - `pub method open` :44
      - `method conn` :69
      - `method ensure_branch` :74
      - `method apply_diff` :85
      - `method lookup_symbol` :343
      - `method find_callers` :364
      - `method find_callers_deep` :382
      - `method symbol_context` :426
      - `method list_definitions` :478
      - `method branch_diff` :494
      - `method list_all_nodes` :534
      - `method list_all_edges` :544
      - `method find_callees` :571
      - `method find_implementors` :618
      - `method trace_path` :635
      - `method list_symbols_in_range` :702
      - `method find_unused_symbols` :727
      - `method get_subgraph` :752
      - `method last_indexed_sha` :876
      - `method set_last_indexed_sha` :880

## crates/gitcortex-store/src/kuzu/conv.rs


## crates/gitcortex-store/src/kuzu/escape.rs


## crates/gitcortex-store/src/kuzu/mod.rs


## crates/gitcortex-store/src/kuzu/queries.rs


## crates/gitcortex-store/src/kuzu/values.rs


## crates/gitcortex-store/src/lib.rs


## crates/gitcortex-store/src/memory.rs


## crates/gitcortex-store/src/schema.rs


## crates/gitcortex-store/tests


## crates/gitcortex-store/tests/round_trip.rs


## crates/gitcortex-viz


## crates/gitcortex-viz/build.rs


## crates/gitcortex-viz/src


## crates/gitcortex-viz/src/lib.rs


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


## viz/src

- `pub folder src` :1
  - `pub file api.ts` :1
    - `pub trait RawNode` :1
    - `pub trait RawEdge` :15
    - `pub trait GraphData` :21
    - `pub async function fetchGraphData` :26
    - `pub trait DeepCallersHop` :32
    - `pub trait DeepCallersResult` :37
    - `pub async function fetchDeepCallers` :44
    - `pub trait BranchListResult` :50
    - `pub trait UnusedResult` :56
    - `pub async function fetchUnused` :61
    - `pub async function fetchBranches` :70
    - `pub trait BranchDiffResult` :76
    - `pub async function fetchBranchDiff` :83
  - `pub file App.tsx` :1
    - `pub function App` :14
  - `pub folder hooks` :1
    - `pub file useBranchDiff.ts` :1
      - `pub trait DiffOverlay` :4
      - `pub function useBranchDiff` :11
  - `pub folder components` :1
    - `pub file KeyboardHelp.tsx` :1
      - `pub trait Props` :3
      - `pub constant SHORTCUTS` :7
      - `pub function KeyboardHelp` :22
    - `pub file BranchPicker.tsx` :1
      - `pub trait Props` :5
      - `pub function BranchPicker` :11
    - `pub file FilterRail.tsx` :1
      - `pub type_alias Visibility` :6
      - `pub type_alias Flag` :7
      - `pub constant VIS_LABEL` :9
      - `pub trait Props` :15
      - `pub function FilterRail` :28
      - `pub function FilterSection` :148
      - `pub function FilterRow` :159
    - `pub file Header.tsx` :1
      - `pub trait Props` :5
      - `pub constant DENSITY_OPTIONS` :19
      - `pub function Header` :21
    - `pub file SearchPalette.tsx` :1
      - `pub trait Props` :6
      - `pub function SearchPalette` :12
    - `pub file Inspector.tsx` :1
      - `pub type_alias Tab` :7
      - `pub trait Props` :9
      - `pub constant RISK_TONE` :18
      - `pub function Inspector` :25
      - `pub function DeepCallersPanel` :139
      - `pub function EmptyHint` :185
      - `pub function TabBtn` :189
      - `pub function Badge` :212
      - `pub function NodeList` :228
    - `pub file CanvasControls.tsx` :1
      - `pub trait Props` :5
      - `pub function CanvasControls` :9
      - `pub function Btn` :71
    - `pub file StatusBar.tsx` :1
      - `pub trait Props` :4
      - `pub function StatusBar` :12
    - `pub file CosmosCanvas.tsx` :1
      - `pub constant DIFF_ADDED` :9
      - `pub constant DIFF_REMOVED` :10
      - `pub trait PointRow` :12
      - `pub trait LinkRow` :20
      - `pub trait Props` :28
      - `pub function CosmosCanvas` :39
  - `pub folder graph` :1
    - `pub file density.ts` :1
      - `pub type_alias DensityMode` :3
      - `pub constant DENSITY_LABEL` :5
      - `pub constant SEMANTIC_EDGE_KINDS` :11
      - `pub constant STRUCTURAL_KINDS` :13
      - `pub function applyDensity` :15
      - `pub function filterByIds` :37
    - `pub folder __tests__` :1
      - `pub file density.test.ts` :1
        - `pub function node` :5
        - `pub function edge` :19
        - `pub function buildGraph` :21
  - `pub folder theme` :1
    - `pub file colors.ts` :1
      - `pub constant KIND_COLOR` :1
      - `pub constant EDGE_COLOR` :19
      - `pub constant EDGE_WIDTH` :30
      - `pub constant KIND_LABEL` :41
      - `pub function dimColor` :59

## viz/src/App.tsx


## viz/src/api.ts


## viz/src/components


## viz/src/components/BranchPicker.tsx


## viz/src/components/CanvasControls.tsx


## viz/src/components/CosmosCanvas.tsx


## viz/src/components/FilterRail.tsx


## viz/src/components/Header.tsx


## viz/src/components/Inspector.tsx


## viz/src/components/KeyboardHelp.tsx


## viz/src/components/SearchPalette.tsx


## viz/src/components/StatusBar.tsx


## viz/src/graph


## viz/src/graph/__tests__


## viz/src/graph/__tests__/density.test.ts


## viz/src/graph/density.ts


## viz/src/hooks


## viz/src/hooks/useBranchDiff.ts


## viz/src/theme


## viz/src/theme/colors.ts


