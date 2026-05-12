# Codebase Map

> Branch: `feature/v0-2-x` · 615 definitions · SHA: `b95acff26265740635f623ba035cef4e46bc84cf`

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
  - `pub folder gitcortex-mcp` :1
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
          - `method visit_method` :307
          - `method receiver_type` :341
          - `method visit_type_decl` :366
          - `method visit_const_decl` :419
          - `method collect_imports` :438
          - `method record_import_spec` :466
          - `method collect_interface_assertions` :494
          - `method collect_candidate_type_names` :535
          - `method extract_fn_type_uses` :555
          - `method extract_struct_field_uses` :600
          - `method extract_interface_methods` :635
          - `method collect_generic_bounds` :667
          - `method collect_type_idents` :699
          - `method walk_type_idents` :705
          - `method collect_calls` :724
          - `method callee_name` :751
          - `method record_call` :762
        - `function is_builtin_go_type` :786
        - `module tests` :817
          - `function parse` :823
          - `function parse_full` :834
          - `function parses_function` :856
          - `function parses_struct_and_method` :868
          - `function parses_interface` :889
          - `function go_visibility_is_uppercase` :898
          - `function detects_call_edges` :909
          - `function package_node_is_emitted` :917
          - `function detects_import_declaration` :929
          - `function detects_fn_type_uses` :943
          - `function detects_interface_assertion` :957
          - `function captures_interface_methods` :967
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
          - `method visit_interface_nested` :391
          - `method visit_interface` :430
          - `method visit_enum` :477
          - `method visit_record` :515
          - `method visit_method` :543
          - `method collect_imports` :609
          - `method extract_annotation_uses` :639
          - `method has_functional_interface_annotation` :664
          - `method extract_field_uses` :685
          - `method extract_simple_type` :694
          - `method collect_type_names` :712
          - `method walk_type_names` :718
          - `method collect_calls` :739
          - `method callee_name` :758
          - `method record_call` :780
        - `function is_builtin_java_type` :804
        - `module tests` :859
          - `function parse` :865
          - `function parse_full` :878
          - `function parses_class_and_method` :908
          - `function parses_interface` :930
          - `function parses_enum` :942
          - `function detects_extends_and_implements` :951
          - `function detects_type_annotation_uses` :963
          - `function detects_import_declaration` :974
          - `function module_node_is_emitted` :984
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
          - `method visit_namespace` :566
          - `method visit_var_decl` :593
          - `method collect_imports` :648
          - `method extract_generic_constraints` :710
          - `method extract_param_types` :727
          - `method extract_return_type_annotation` :743
          - `method extract_heritage` :754
          - `method collect_extends_names` :777
          - `method collect_implements_names` :798
          - `method extract_decorator_annotated` :816
          - `method extract_decorator_uses` :828
          - `method decorator_name` :840
          - `method collect_type_names` :864
          - `method walk_type_names` :870
          - `method collect_calls` :891
          - `method callee_name` :909
          - `method record_call` :920
        - `function is_builtin_ts_type` :944
        - `module tests` :992
          - `function parse_ts` :998
          - `function parse_ts_full` :1011
          - `function parse_js` :1036
          - `function parses_ts_function` :1049
          - `function parses_ts_class_and_method` :1061
          - `function parses_ts_interface` :1082
          - `function parses_js_arrow_function` :1090
          - `function detects_ts_call_edges` :1101
          - `function detects_ts_extends_implements` :1109
          - `function detects_ts_type_annotation_uses` :1121
          - `function detects_ts_named_imports` :1132
          - `function module_node_is_emitted` :1146
      - `pub file mod.rs` :1
        - `pub module go` :8
        - `pub module java` :9
        - `pub module python` :10
        - `pub module rust` :11
        - `pub module typescript` :12
        - `pub struct ParseResult` :15
        - `pub trait LanguageParser` :40
        - `pub function parser_for_path` :51

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

## crates/gitcortex-mcp/tests/full_pipeline.rs


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
      - `pub file sample.rs` :1
        - `pub trait Greeter` :1
        - `pub struct Hello` :5
          - `method greet` :10
        - `pub function make_greeting` :15
      - `pub file sample.py` :1
        - `pub module sample` :1
        - `pub struct Greeter` :1
          - `pub method greet` :2
        - `pub struct FancyGreeter` :6
          - `pub method greet` :2
        - `pub function make_greeting` :11
      - `pub file sample.java` :1
        - `pub module sample` :1
        - `pub(crate) function Greeter` :3
          - `pub(crate) method greet` :4
        - `pub(crate) struct Hello` :7
          - `pub method greet` :8
        - `pub(crate) struct GreetingFactory` :14
          - `pub method makeGreeting` :15
      - `pub file sample.ts` :1
        - `pub module sample` :1
        - `pub trait Greeter` :1
          - `pub method greet` :2
        - `pub struct Hello` :5
          - `pub method greet` :6
        - `pub struct FancyGreeter` :11
          - `pub struct Hello` :5
            - `method greet` :10
          - `pub struct Hello` :5
            - `pub method greet` :6
          - `pub struct Hello` :7
            - `pub method Greet` :11
          - `pub(crate) struct Hello` :7
            - `pub method greet` :8
          - `pub method greet` :12
        - `pub function makeGreeting` :17
      - `pub file sample.go` :1
        - `pub module greeter` :1
        - `pub trait Greeter` :3
          - `pub method Greet` :4
        - `pub struct Hello` :7
          - `pub method Greet` :11
        - `pub function MakeGreeting` :15

## tests/integration


## tests/integration/fixtures


## tests/integration/fixtures/sample.go


## tests/integration/fixtures/sample.java


## tests/integration/fixtures/sample.py


## tests/integration/fixtures/sample.rs


## tests/integration/fixtures/sample.ts


