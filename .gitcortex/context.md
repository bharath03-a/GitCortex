# Codebase Map

> Branch: `feature/v0-2-x` · 839 definitions · SHA: `c5599c8c4cf7d0e7bb17aed21a71e47c30ba1111`

## crates

- `pub folder crates` :1
  - `pub folder gitcortex-cli` :1
    - `pub folder src` :1
      - `pub file main.rs` :1
        - `module cmd` :1
        - `struct Cli` :11
        - `enum Commands` :17
        - `pub enum QueryCmd` :88
        - `function main` :180
      - `pub folder cmd` :1
        - `pub file query.rs` :1
          - `pub function run` :10
          - `function parse_node_kind` :274
          - `function repo_root` :287
        - `pub file update.rs` :1
          - `constant CURRENT` :3
          - `constant RELEASES_API` :4
          - `pub function run` :6
          - `function fetch_latest_version` :30
          - `enum InstallMethod` :67
            - `method fmt` :75
          - `function detect_install_method` :85
          - `function update_command` :103
        - `pub file hook.rs` :1
          - `pub function run` :11
          - `function repo_root` :77
          - `function current_branch` :87
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
        - `pub file export.rs` :1
          - `constant DEFAULT_OUTPUT` :15
          - `pub function run` :17
          - `pub function refresh_if_exists` :32
          - `function write_context` :41
          - `function build_context_md` :66
          - `function render_node` :126
          - `function repo_root` :168
          - `function current_branch` :181
        - `pub file clean.rs` :1
          - `pub function run` :8
          - `function repo_root` :26
        - `pub file status.rs` :1
          - `pub function run` :7
          - `function repo_root` :50
          - `function current_branch` :60
        - `pub file serve.rs` :1
          - `pub function run` :3
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
            - `pub file windsurf.rs` :1
              - `constant WINDSURF_RULES` :8
              - `pub function install` :37
              - `function write_windsurf_rules` :43
              - `function write_windsurf_mcp` :58
            - `pub file cursor.rs` :1
              - `constant CURSOR_RULES` :6
              - `pub function install` :44
              - `function write_cursor_rules` :50
              - `function write_cursor_mcp` :60
            - `pub file copilot.rs` :1
              - `constant COPILOT_INSTRUCTIONS` :5
              - `pub function install` :38
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
  - `pub folder gitcortex-core` :1
    - `pub folder src` :1
      - `pub file store.rs` :1
        - `pub struct SubGraph` :10
        - `pub struct CallersDeep` :16
        - `pub struct SymbolContext` :24
        - `pub trait GraphStore` :40
      - `pub file lib.rs` :1
        - `pub module error` :1
        - `pub module graph` :2
        - `pub module schema` :3
        - `pub module store` :4
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
  - `pub folder gitcortex-indexer` :1
    - `pub folder src` :1
      - `pub file differ.rs` :1
        - `pub enum FileChange` :10
          - `pub method path` :17
        - `pub struct Differ` :27
          - `pub method open` :33
          - `pub method head_sha` :40
          - `pub method changed_files` :57
      - `pub file lib.rs` :1
        - `pub module differ` :1
        - `pub module indexer` :2
        - `pub module parser` :3
      - `pub file indexer.rs` :1
        - `type_alias FileIndexResult` :20
        - `pub struct IncrementalIndexer` :35
          - `pub method new` :44
          - `pub method run` :59
          - `method supported_extensions` :192
          - `method index_file` :198
          - `method should_ignore` :268
        - `function resolve_deferred` :286
        - `function language_extensions_for_path` :335
        - `function build_structural_nodes` :357
        - `function build_ignorer` :480
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
            - `method collect_names` :195
            - `method visit_module` :247
            - `method visit_top_level` :255
            - `method visit_function` :283
            - `method visit_class` :356
            - `method maybe_visit_constant` :491
            - `method collect_imports` :517
            - `method collect_calls` :573
            - `method callee_name` :590
            - `method record_call` :601
            - `method fn_is_async` :626
            - `method body_has_yield` :634
            - `method collect_decorators` :649
            - `method decorator_name` :658
            - `method extract_param_types` :681
            - `method extract_return_type` :699
            - `method collect_type_names` :708
            - `method walk_type_names` :714
          - `function is_builtin_type` :734
          - `module tests` :793
            - `function parse` :799
            - `function parse_full` :812
            - `function parses_free_function` :836
            - `function parses_class_and_method` :849
            - `function detects_call_edges` :870
            - `function detects_base_class_implements` :878
            - `function detects_type_annotation_uses` :888
            - `function detects_decorator_uses` :899
            - `function detects_import_statement` :909
            - `function detects_from_import_statement` :923
            - `function module_node_is_emitted` :937
            - `function async_function_flagged` :948
            - `function protocol_class_becomes_interface` :962
            - `function non_protocol_class_is_struct` :978
            - `function property_decorator_yields_property_kind` :993
            - `function staticmethod_decorator_sets_is_static` :1007
            - `function classmethod_decorator_sets_is_static` :1023
            - `function dataclass_decorator_class_is_struct` :1039
            - `function generator_function_sets_is_generator` :1053
            - `function async_generator_is_both_async_and_generator` :1069
            - `function nested_yield_does_not_pollute_outer_function` :1088
            - `function module_constant_all_caps_detected` :1109
            - `function nested_class_emits_contains_edge_from_parent` :1131
            - `function multiple_type_annotations_produce_uses_entries` :1157
            - `function private_method_has_private_visibility` :1170
            - `function calls_edge_between_two_functions` :1189
            - `function method_call_via_self_creates_calls_edge` :1197
            - `function aliased_import_uses_alias_name` :1219
            - `function dotted_import_records_leaf_module` :1235
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
            - `method collect_type_names` :719
            - `method walk_type_names` :725
            - `method collect_calls` :746
            - `method callee_name` :765
            - `method record_call` :787
          - `function is_builtin_java_type` :811
          - `module tests` :866
            - `function parse` :872
            - `function parse_full` :885
            - `function parses_class_and_method` :915
            - `function parses_interface` :937
            - `function parses_enum` :949
            - `function detects_extends_and_implements` :958
            - `function detects_type_annotation_uses` :970
            - `function detects_import_declaration` :981
            - `function module_node_is_emitted` :991
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
            - `method type_name` :276
            - `method collect_names` :295
            - `method visit_items` :327
            - `method visit_item` :335
            - `method visit_function` :350
            - `method collect_uses_edges` :399
            - `method collect_calls` :445
            - `method callee_name` :489
            - `method record_call` :506
            - `method visit_type_item` :528
            - `method visit_trait` :557
            - `method visit_impl` :586
            - `method visit_mod` :631
            - `method visit_const` :653
            - `method visit_type_alias` :669
            - `method visit_macro_def` :690
            - `method collect_imports` :713
            - `method collect_import_leaves` :729
          - `function is_primitive` :768
          - `module tests` :839
            - `function parse` :850
            - `function parses_free_function` :856
            - `function parses_struct` :864
            - `function parses_trait_impl_and_method` :875
            - `function parses_module_with_items` :906
            - `function qualified_name_includes_module_path` :934
            - `function detects_intra_file_calls` :946
            - `function detects_uses_edges_for_param_types` :957
            - `function deferred_calls_capture_unknown_callees` :968
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
            - `method collect_names` :248
            - `method collect_names_from_var_decl` :280
            - `method unwrap_export` :303
            - `method visit_program` :326
            - `method visit_statement` :335
            - `method visit_function` :367
            - `method visit_class` :407
            - `method visit_method` :449
            - `method visit_interface` :476
            - `method visit_type_alias` :507
            - `method visit_enum` :521
            - `method visit_namespace` :567
            - `method visit_var_decl` :594
            - `method collect_imports` :649
            - `method extract_generic_constraints` :711
            - `method extract_param_types` :728
            - `method extract_return_type_annotation` :744
            - `method extract_heritage` :755
            - `method collect_extends_names` :778
            - `method collect_implements_names` :799
            - `method extract_decorator_annotated` :817
            - `method extract_decorator_uses` :829
            - `method decorator_name` :841
            - `method collect_type_names` :865
            - `method walk_type_names` :871
            - `method collect_calls` :892
            - `method callee_name` :910
            - `method record_call` :921
          - `function is_builtin_ts_type` :945
          - `module tests` :993
            - `function parse_ts` :999
            - `function parse_ts_full` :1012
            - `function parse_js` :1037
            - `function parses_ts_function` :1050
            - `function parses_ts_class_and_method` :1062
            - `function parses_ts_interface` :1083
            - `function parses_js_arrow_function` :1091
            - `function detects_ts_call_edges` :1102
            - `function detects_ts_extends_implements` :1110
            - `function detects_ts_type_annotation_uses` :1122
            - `function detects_ts_named_imports` :1133
            - `function module_node_is_emitted` :1147
        - `pub file deftext.rs` :1
          - `constant MAX_BODY_BYTES` :12
          - `pub(crate) function capture` :23
          - `function extract_signature` :52
          - `function truncate_to_char_boundary` :75
          - `function preceding_doc_comment` :91
          - `function inline_docstring` :129
          - `function is_doc_style` :158
          - `module tests` :172
            - `function signature_stops_at_brace` :176
            - `function signature_python_def` :184
            - `function signature_falls_back_to_first_line` :192
            - `function python_docstring_extracted` :197
            - `function python_docstring_single_quotes` :206
            - `function no_docstring_returns_none` :212
            - `function truncate_respects_char_boundary` :218
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
            - `method collect_names` :215
            - `method collect_type_decl_names` :240
            - `method visit_source_file` :259
            - `method visit_top_level` :267
            - `method visit_function` :277
            - `method visit_method` :308
            - `method receiver_type` :342
            - `method visit_type_decl` :367
            - `method visit_const_decl` :420
            - `method collect_imports` :439
            - `method record_import_spec` :467
            - `method collect_interface_assertions` :495
            - `method collect_candidate_type_names` :536
            - `method extract_fn_type_uses` :556
            - `method extract_struct_field_uses` :601
            - `method extract_interface_methods` :636
            - `method collect_generic_bounds` :668
            - `method collect_type_idents` :700
            - `method walk_type_idents` :706
            - `method collect_calls` :725
            - `method callee_name` :752
            - `method record_call` :763
          - `function is_builtin_go_type` :787
          - `module tests` :818
            - `function parse` :824
            - `function parse_full` :835
            - `function parses_function` :857
            - `function parses_struct_and_method` :869
            - `function parses_interface` :890
            - `function go_visibility_is_uppercase` :899
            - `function detects_call_edges` :910
            - `function package_node_is_emitted` :918
            - `function detects_import_declaration` :930
            - `function detects_fn_type_uses` :944
            - `function detects_interface_assertion` :958
            - `function captures_interface_methods` :968
        - `pub file mod.rs` :1
          - `module deftext` :8
          - `pub module go` :9
          - `pub module java` :10
          - `pub module python` :11
          - `pub module rust` :12
          - `pub module typescript` :13
          - `pub struct ParseResult` :18
          - `pub trait LanguageParser` :43
          - `pub function parser_for_path` :54
  - `pub folder gitcortex-store` :1
    - `pub folder src` :1
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
      - `pub file schema.rs` :1
        - `pub function node_table` :9
        - `pub function edge_table` :14
        - `pub function ensure_branch` :22
      - `pub file lib.rs` :1
        - `pub module branch` :1
        - `pub module kuzu` :4
        - `pub module schema` :6
        - `pub module memory` :9
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
            - `method lookup_symbol` :402
            - `method find_callers` :423
            - `method find_callers_deep` :441
            - `method symbol_context` :485
            - `method list_definitions` :548
            - `method branch_diff` :564
            - `method list_all_nodes` :604
            - `method list_all_edges` :614
            - `method find_callees` :641
            - `method find_implementors` :688
            - `method trace_path` :705
            - `method list_symbols_in_range` :772
            - `method find_unused_symbols` :797
            - `method get_subgraph` :822
            - `method last_indexed_sha` :946
            - `method set_last_indexed_sha` :950
        - `pub file escape.rs` :1
          - `pub function esc` :6
          - `pub function esc_multiline` :13
        - `pub file queries.rs` :1
          - `pub constant NODE_COLS` :19
          - `pub function rows_to_nodes` :24
          - `pub function row_to_node` :35
          - `pub function collect_ids` :111
        - `pub file conv.rs` :1
          - `pub function kind_from_str` :7
          - `pub function edge_kind_from_str` :28
          - `pub function vis_str` :41
          - `pub function vis_from_str` :49
          - `pub function language_extensions` :63
          - `pub function lang_scope_clause` :81
        - `pub file values.rs` :1
          - `pub function str_val` :8
          - `pub function i64_val` :20
          - `pub function bool_val` :30
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
  - `pub folder gitcortex-mcp` :1
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
    - `pub folder src` :1
      - `pub file lib.rs` :1
        - `pub module mcp` :1
      - `pub folder mcp` :1
        - `pub file server.rs` :1
          - `pub function serve` :8
        - `pub file search.rs` :1
          - `pub struct SearchHit` :17
          - `constant DEFAULT_LIMIT` :27
          - `constant MAX_LIMIT` :28
          - `pub function search` :33
          - `function score` :64
          - `function kind_boost` :85
          - `function to_hit` :95
        - `pub file wiki.rs` :1
          - `pub function render_symbol` :40
          - `function format` :49
          - `function write_neighbor_list` :96
          - `function strip_doc_markers` :116
          - `function file_lang` :144
          - `module tests` :158
            - `function lang_from_path` :162
            - `function strip_rust_doc_markers` :169
        - `pub file tour.rs` :1
          - `pub struct TourStep` :23
          - `pub struct Tour` :36
          - `constant DEFAULT_TOUR_LEN` :43
          - `constant MAX_TOUR_LEN` :45
          - `pub function generate` :50
          - `function global_tour` :90
          - `function seeded_tour` :135
          - `pub function render_markdown` :193
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
          - `pub struct WikiSymbolParams` :125
          - `pub struct SearchCodeParams` :132
          - `pub struct StartTourParams` :141
          - `pub struct GitCortexServer` :156
            - `pub method new` :163
            - `method lookup_symbol` :201
            - `method find_callers` :243
            - `method symbol_context` :326
            - `method list_definitions` :371
            - `method branch_diff_graph` :408
            - `method detect_changes` :463
            - `method find_callees` :543
            - `method find_implementors` :591
            - `method trace_path` :633
            - `method list_symbols_in_range` :673
            - `method find_unused_symbols` :718
            - `method get_subgraph` :772
            - `method wiki_symbol` :830
            - `method search_code` :857
            - `method start_tour` :885
            - `method detect_impact` :937
            - `method generate_map` :973
          - `function detect_current_branch` :174
          - `pub struct DetectImpactParams` :913
          - `pub struct GenerateMapParams` :921
          - `function run_git_diff` :1024
          - `function parse_diff_hunks` :1038
          - `function parse_hunk_header` :1067
        - `pub file mod.rs` :1
          - `pub module search` :1
          - `pub module server` :2
          - `pub module tools` :3
          - `pub module tour` :4
          - `pub module wiki` :5

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


## crates/gitcortex-indexer/src/parser/deftext.rs


## crates/gitcortex-indexer/src/parser/go.rs


## crates/gitcortex-indexer/src/parser/java.rs


## crates/gitcortex-indexer/src/parser/mod.rs


## crates/gitcortex-indexer/src/parser/python.rs


## crates/gitcortex-indexer/src/parser/rust.rs


## crates/gitcortex-indexer/src/parser/typescript.rs


## crates/gitcortex-mcp


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


## crates/gitcortex-mcp/tests/full_pipeline.rs


## crates/gitcortex-store


## crates/gitcortex-store/src


## crates/gitcortex-store/src/branch.rs


## crates/gitcortex-store/src/kuzu


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
      - `pub file sample.go` :1
        - `pub trait Greeter` :3
        - `pub struct Hello` :7
          - `pub method Greet` :11
        - `pub function MakeGreeting` :15
      - `pub file sample.ts` :1
        - `pub trait Greeter` :1
        - `pub struct Hello` :5
          - `pub method greet` :6
        - `pub struct FancyGreeter` :11
          - `pub method greet` :12
        - `pub function makeGreeting` :17
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
    - `pub folder hooks` :1
      - `pub file useBranchDiff.ts` :1
        - `pub trait DiffOverlay` :4
        - `pub function useBranchDiff` :11
    - `pub folder components` :1
      - `pub file FilterRail.tsx` :1
        - `pub type_alias Visibility` :6
        - `pub type_alias Flag` :7
        - `pub constant VIS_LABEL` :9
        - `pub trait Props` :15
        - `pub function FilterRail` :28
        - `pub function FilterSection` :148
        - `pub function FilterRow` :159
      - `pub file StatusBar.tsx` :1
        - `pub trait Props` :4
        - `pub function StatusBar` :12
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
      - `pub file Header.tsx` :1
        - `pub trait Props` :5
        - `pub constant DENSITY_OPTIONS` :19
        - `pub function Header` :21
      - `pub file CosmosCanvas.tsx` :1
        - `pub constant DIFF_ADDED` :9
        - `pub constant DIFF_REMOVED` :10
        - `pub trait PointRow` :12
        - `pub trait LinkRow` :20
        - `pub trait Props` :28
        - `pub function CosmosCanvas` :39
      - `pub file SearchPalette.tsx` :1
        - `pub trait Props` :6
        - `pub function SearchPalette` :12
      - `pub file BranchPicker.tsx` :1
        - `pub trait Props` :5
        - `pub function BranchPicker` :11
      - `pub file KeyboardHelp.tsx` :1
        - `pub trait Props` :3
        - `pub constant SHORTCUTS` :7
        - `pub function KeyboardHelp` :22
    - `pub folder theme` :1
      - `pub file colors.ts` :1
        - `pub constant KIND_COLOR` :1
        - `pub constant EDGE_COLOR` :19
        - `pub constant EDGE_WIDTH` :30
        - `pub constant KIND_LABEL` :41
        - `pub function dimColor` :59
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


