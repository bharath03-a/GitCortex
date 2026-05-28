# Codebase Map

> Branch: `feature/v0-3-x` · 865 definitions · SHA: `c60af2e139e595a411eb712a2515446809fd25b9`

## crates

- `pub folder crates` :1
  - `pub folder gitcortex-cli` :1
    - `pub folder src` :1
      - `pub file main.rs` :1
        - `module cmd` :1
        - `struct Cli` :11
        - `enum Commands` :17
        - `pub enum QueryCmd` :99
        - `function main` :191
      - `pub folder cmd` :1
        - `pub file export.rs` :1
          - `constant DEFAULT_OUTPUT` :15
          - `constant CLAUDE_MD` :16
          - `constant SYMBOLS_BEGIN` :17
          - `constant SYMBOLS_END` :18
          - `pub enum ExportFormat` :22
          - `pub function run` :30
          - `pub function refresh_if_exists` :66
          - `function write_context` :75
          - `function build_symbols_json` :103
          - `function vis_str` :148
          - `function write_claude_md` :162
          - `function upsert_block` :245
          - `function build_context_md` :265
          - `function render_node` :325
          - `function repo_root` :367
          - `function current_branch` :380

## crates/gitcortex-cli


## crates/gitcortex-cli/src


## crates/gitcortex-cli/src/cmd


## crates/gitcortex-cli/src/cmd/blast_radius.rs


## crates/gitcortex-cli/src/cmd/clean.rs


## crates/gitcortex-cli/src/cmd/doctor.rs


## crates/gitcortex-cli/src/cmd/export.rs


## crates/gitcortex-cli/src/cmd/hook.rs


## crates/gitcortex-cli/src/cmd/init

- `pub folder init` :1
  - `pub file helpers.rs` :1
    - `pub function repo_root` :5
    - `pub function home_dir` :18
    - `pub function current_branch` :25
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
  - `pub folder editors` :1
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
    - `pub file windsurf.rs` :1
      - `constant WINDSURF_RULES` :8
      - `pub function install` :37
      - `function write_windsurf_rules` :43
      - `function write_windsurf_mcp` :58
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
    - `pub file antigravity.rs` :1
      - `pub function install` :8
      - `function write_antigravity_mcp` :13
    - `pub file copilot.rs` :1
      - `constant COPILOT_INSTRUCTIONS` :5
      - `pub function install` :38

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
    - `pub file lib.rs` :1
      - `pub module error` :1
      - `pub module graph` :2
      - `pub module schema` :3
      - `pub module store` :4
    - `pub file store.rs` :1
      - `pub struct SubGraph` :10
      - `pub struct CallersDeep` :16
      - `pub struct SymbolContext` :24
      - `pub trait GraphStore` :40
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
    - `pub file graph.rs` :1
      - `pub struct NodeId` :15
        - `pub method new` :18
        - `pub method as_str` :22
        - `method default` :28
        - `method fmt` :34
        - `method try_from` :42
      - `pub struct Span` :52
      - `pub struct LldLabels` :62
      - `pub struct DefinitionText` :77
      - `pub struct NodeMetadata` :93
      - `pub struct Node` :124
      - `pub struct Edge` :139
      - `pub struct GraphDiff` :150
        - `pub method is_empty` :178
        - `pub method merge` :195
      - `module tests` :213
        - `function node_id_is_unique` :217
        - `function graph_diff_merge` :224
        - `function graph_diff_is_empty_on_default` :247
    - `pub file error.rs` :1
      - `pub enum GitCortexError` :11
      - `pub type_alias Result` :33

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
          - `method is_type_member` :222
          - `method is_exported` :237
          - `method is_async` :256
          - `method qualified` :262
          - `method make_node` :270
          - `method collect_names` :298
          - `method collect_names_from_var_decl` :330
          - `method unwrap_export` :353
          - `method visit_program` :376
          - `method visit_statement` :385
          - `method visit_function` :417
          - `method visit_class` :457
          - `method visit_method` :499
          - `method visit_interface` :526
          - `method visit_type_alias` :557
          - `method visit_enum` :571
          - `method visit_namespace` :617
          - `method visit_var_decl` :644
          - `method collect_imports` :699
          - `method extract_generic_constraints` :761
          - `method extract_param_types` :778
          - `method extract_return_type_annotation` :794
          - `method extract_heritage` :805
          - `method collect_extends_names` :828
          - `method collect_implements_names` :849
          - `method extract_decorator_annotated` :867
          - `method extract_decorator_uses` :879
          - `method decorator_name` :891
          - `method collect_type_names` :915
          - `method walk_type_names` :921
          - `method collect_calls` :942
          - `method callee_name` :960
          - `method record_call` :971
        - `function is_builtin_ts_type` :995
        - `module tests` :1043
          - `function parse_ts` :1049
          - `function parse_ts_full` :1062
          - `function parse_js` :1087
          - `function parses_ts_function` :1100
          - `function parses_ts_class_and_method` :1112
          - `function parses_ts_interface` :1133
          - `function parses_js_arrow_function` :1141
          - `function detects_ts_call_edges` :1152
          - `function detects_ts_extends_implements` :1160
          - `function detects_ts_type_annotation_uses` :1172
          - `function detects_ts_named_imports` :1183
          - `function module_node_is_emitted` :1197
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
          - `method collect_names` :195
          - `method visit_module` :247
          - `method visit_top_level` :255
          - `method visit_function` :283
          - `method visit_class` :356
          - `method maybe_visit_constant` :491
          - `method collect_imports` :523
          - `method collect_calls` :579
          - `method callee_name` :596
          - `method record_call` :607
          - `method fn_is_async` :632
          - `method body_has_yield` :640
          - `method collect_decorators` :655
          - `method decorator_name` :664
          - `method extract_param_types` :687
          - `method extract_return_type` :705
          - `method collect_type_names` :714
          - `method walk_type_names` :720
        - `function is_builtin_type` :740
        - `module tests` :799
          - `function parse` :805
          - `function parse_full` :818
          - `function parses_free_function` :842
          - `function parses_class_and_method` :855
          - `function detects_call_edges` :876
          - `function detects_base_class_implements` :884
          - `function detects_type_annotation_uses` :894
          - `function detects_decorator_uses` :905
          - `function detects_import_statement` :915
          - `function detects_from_import_statement` :929
          - `function module_node_is_emitted` :943
          - `function async_function_flagged` :954
          - `function protocol_class_becomes_interface` :968
          - `function non_protocol_class_is_struct` :984
          - `function property_decorator_yields_property_kind` :999
          - `function staticmethod_decorator_sets_is_static` :1013
          - `function classmethod_decorator_sets_is_static` :1029
          - `function dataclass_decorator_class_is_struct` :1045
          - `function generator_function_sets_is_generator` :1059
          - `function async_generator_is_both_async_and_generator` :1075
          - `function nested_yield_does_not_pollute_outer_function` :1094
          - `function module_level_bindings_detected` :1115
          - `function nested_class_emits_contains_edge_from_parent` :1147
          - `function multiple_type_annotations_produce_uses_entries` :1173
          - `function private_method_has_private_visibility` :1186
          - `function calls_edge_between_two_functions` :1205
          - `function method_call_via_self_creates_calls_edge` :1213
          - `function aliased_import_uses_alias_name` :1235
          - `function dotted_import_records_leaf_module` :1251
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
          - `method collect_names` :226
          - `method visit_program` :248
          - `method visit_top_level` :256
          - `method visit_class` :266
          - `method visit_class_nested` :343
          - `method visit_interface_nested` :392
          - `method visit_interface` :431
          - `method visit_enum` :478
          - `method visit_record` :516
          - `method visit_method` :544
          - `method collect_imports` :616
          - `method extract_annotation_uses` :646
          - `method has_functional_interface_annotation` :671
          - `method extract_field_uses` :692
          - `method extract_simple_type` :701
          - `method collect_type_names` :733
          - `method walk_type_names` :739
          - `method collect_calls` :760
          - `method callee_name` :779
          - `method record_call` :801
        - `function is_builtin_java_type` :825
        - `module tests` :880
          - `function parse` :886
          - `function parse_full` :899
          - `function parses_class_and_method` :929
          - `function parses_interface` :951
          - `function parses_enum` :963
          - `function detects_extends_and_implements` :972
          - `function detects_type_annotation_uses` :984
          - `function detects_import_declaration` :995
          - `function module_node_is_emitted` :1005

## crates/gitcortex-indexer/src


## crates/gitcortex-indexer/src/differ.rs


## crates/gitcortex-indexer/src/indexer.rs


## crates/gitcortex-indexer/src/lib.rs


## crates/gitcortex-indexer/src/parser


## crates/gitcortex-indexer/src/parser/deftext.rs


## crates/gitcortex-indexer/src/parser/go.rs


## crates/gitcortex-indexer/src/parser/java.rs


## crates/gitcortex-indexer/src/parser/mod.rs


## crates/gitcortex-indexer/src/parser/python.rs


## crates/gitcortex-indexer/src/parser/rust.rs


## crates/gitcortex-indexer/src/parser/typescript.rs


## crates/gitcortex-mcp

- `pub folder gitcortex-mcp` :1
  - `pub folder src` :1
    - `pub folder mcp` :1
      - `pub file server.rs` :1
        - `pub function serve` :8

## crates/gitcortex-mcp/src


## crates/gitcortex-mcp/src/lib.rs


## crates/gitcortex-mcp/src/mcp


## crates/gitcortex-mcp/src/mcp/mod.rs


## crates/gitcortex-mcp/src/mcp/search.rs


## crates/gitcortex-mcp/src/mcp/server.rs


## crates/gitcortex-mcp/src/mcp/tools.rs


## crates/gitcortex-mcp/src/mcp/tour.rs


## crates/gitcortex-mcp/src/mcp/wiki.rs


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


## crates/gitcortex-store

- `pub folder gitcortex-store` :1
  - `pub folder src` :1
    - `pub folder kuzu` :1
      - `pub file queries.rs` :1
        - `pub constant NODE_COLS` :19
        - `pub constant SYMBOL_RANK` :32
        - `pub function rows_to_nodes` :40
        - `pub function row_to_node` :51
        - `pub function collect_ids` :127
      - `pub file mod.rs` :1
        - `module bulk` :16
        - `module conv` :17
        - `module escape` :18
        - `module queries` :19
        - `module values` :20
        - `constant NODE_INSERT_CHUNK` :30
        - `constant EDGE_INSERT_CHUNK` :31
        - `function node_struct_literal` :36
        - `function node_table_is_empty` :73
        - `function bulk_apply` :88
        - `pub struct KuzuGraphStore` :121
          - `pub method open` :132
          - `method conn` :157
          - `method ensure_branch` :162
          - `method apply_diff` :173
          - `method lookup_symbol` :505
          - `method find_callers` :526
          - `method find_callers_deep` :544
          - `method symbol_context` :588
          - `method list_definitions` :654
          - `method branch_diff` :670
          - `method list_all_nodes` :710
          - `method list_all_edges` :720
          - `method find_callees` :747
          - `method find_implementors` :794
          - `method trace_path` :811
          - `method list_symbols_in_range` :878
          - `method find_unused_symbols` :903
          - `method get_subgraph` :928
          - `method last_indexed_sha` :1052
          - `method set_last_indexed_sha` :1056

## crates/gitcortex-store/src


## crates/gitcortex-store/src/branch.rs


## crates/gitcortex-store/src/kuzu


## crates/gitcortex-store/src/kuzu/bulk.rs


## crates/gitcortex-store/src/kuzu/conv.rs


## crates/gitcortex-store/src/kuzu/escape.rs


## crates/gitcortex-store/src/kuzu/mod.rs


## crates/gitcortex-store/src/kuzu/queries.rs


## crates/gitcortex-store/src/kuzu/values.rs


## crates/gitcortex-store/src/lib.rs


## crates/gitcortex-store/src/memory.rs


## crates/gitcortex-store/src/schema.rs


## crates/gitcortex-store/tests

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

## crates/gitcortex-store/tests/round_trip.rs


## crates/gitcortex-viz

- `pub folder gitcortex-viz` :1
  - `pub file build.rs` :1
    - `function main` :12
    - `function ensure_placeholder` :63
    - `function which` :86
  - `pub folder src` :1
    - `pub file lib.rs` :1
      - `constant VIZ_INDEX` :18
      - `constant VIZ_JS` :19
      - `constant VIZ_CSS` :20
      - `constant VIZ_WEBGL` :21
      - `pub enum VizFormat` :25
      - `struct AppState` :36
      - `pub function run` :41
      - `function serve` :66
      - `function host_header_guard` :121
      - `function root_handler` :151
      - `function js_handler` :155
      - `function css_handler` :159
      - `function webgl_handler` :163
      - `function static_response` :167
      - `function run_blocking` :173
      - `function with_locked_store` :191
      - `function node_json` :202
      - `function data_handler` :221
      - `struct DepthQuery` :255
      - `function symbol_context_handler` :263
      - `function callers_handler` :293
      - `function branches_handler` :335
      - `struct UnusedQuery` :357
      - `function unused_handler` :365
      - `struct BranchDiffQuery` :389
      - `function branch_diff_handler` :395
      - `function parse_node_kind` :420
      - `function list_local_branches_async` :441
      - `function build_dot` :453
      - `function dot_escape` :481
      - `function kind_dot_color` :485
      - `function repo_root` :505

## crates/gitcortex-viz/build.rs


## crates/gitcortex-viz/src


## crates/gitcortex-viz/src/lib.rs


## npm

- `pub folder npm` :1
  - `pub folder packages` :1
    - `pub folder gitcortex` :1
      - `pub folder bin` :1
        - `pub file gcx.js` :1
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
        - `function _binary_path` :9
        - `pub function main` :14

## python/src


## python/src/gitcortex


## python/src/gitcortex/__init__.py


## tests

- `pub folder tests` :1
  - `pub folder integration` :1
    - `pub folder fixtures` :1
      - `pub file sample.go` :1
        - `pub trait Greeter` :3
        - `pub struct Hello` :7
          - `pub method Greet` :11
        - `pub function MakeGreeting` :15
      - `pub file sample.py` :1
        - `pub struct Greeter` :1
          - `pub method greet` :2
        - `pub struct FancyGreeter` :6
          - `pub method greet` :7
        - `pub function make_greeting` :11
      - `pub file sample.rs` :1
        - `pub trait Greeter` :1
        - `pub struct Hello` :5
          - `method greet` :10
        - `pub function make_greeting` :15
      - `pub file sample.ts` :1
        - `pub trait Greeter` :1
        - `pub struct Hello` :5
          - `pub method greet` :6
        - `pub struct FancyGreeter` :11
          - `pub method greet` :12
        - `pub function makeGreeting` :17
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


## tests/integration/fixtures/sample.py


## tests/integration/fixtures/sample.rs


## tests/integration/fixtures/sample.ts


## viz

- `pub folder viz` :1
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
    - `pub folder hooks` :1
      - `pub file useBranchDiff.ts` :1
        - `pub trait DiffOverlay` :4
        - `pub function useBranchDiff` :11
    - `pub folder components` :1
      - `pub file StatusBar.tsx` :1
        - `pub trait Props` :4
        - `pub function StatusBar` :12
      - `pub file BranchPicker.tsx` :1
        - `pub trait Props` :5
        - `pub function BranchPicker` :11
      - `pub file SearchPalette.tsx` :1
        - `pub trait Props` :6
        - `pub function SearchPalette` :12
      - `pub file KeyboardHelp.tsx` :1
        - `pub trait Props` :3
        - `pub constant SHORTCUTS` :7
        - `pub function KeyboardHelp` :22
      - `pub file FilterRail.tsx` :1
        - `pub type_alias Visibility` :6
        - `pub type_alias Flag` :7
        - `pub constant VIS_LABEL` :9
        - `pub trait Props` :15
        - `pub function FilterRail` :28
        - `pub function FilterSection` :148
        - `pub function FilterRow` :159
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
      - `pub file Header.tsx` :1
        - `pub trait Props` :5
        - `pub constant DENSITY_OPTIONS` :19
        - `pub function Header` :21
      - `pub file CanvasControls.tsx` :1
        - `pub trait Props` :5
        - `pub function CanvasControls` :9
        - `pub function Btn` :71
      - `pub file CosmosCanvas.tsx` :1
        - `pub constant DIFF_ADDED` :9
        - `pub constant DIFF_REMOVED` :10
        - `pub trait PointRow` :12
        - `pub trait LinkRow` :20
        - `pub trait Props` :28
        - `pub function CosmosCanvas` :39
    - `pub folder theme` :1
      - `pub file colors.ts` :1
        - `pub constant KIND_COLOR` :1
        - `pub constant EDGE_COLOR` :19
        - `pub constant EDGE_WIDTH` :30
        - `pub constant KIND_LABEL` :41
        - `pub function dimColor` :59

## viz/src


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


