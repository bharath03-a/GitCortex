# Codebase Map

> Branch: `feature/v0-2` · 463 definitions · SHA: `538a738e948143c6c50842774840f5d61743bee4`

## crates

- `pub folder crates` :1
  - `pub folder gitcortex-core` :1
    - `pub folder src` :1
      - `pub file store.rs` :1
        - `pub struct CallersDeep` :9
        - `pub struct SymbolContext` :17
        - `pub trait GraphStore` :33
  - `pub folder gitcortex-store` :1
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
    - `pub folder src` :1
      - `pub file kuzu.rs` :1
        - `pub struct KuzuGraphStore` :23
          - `pub method open` :30
          - `method conn` :46
          - `method ensure_branch` :51
          - `method apply_diff` :62
          - `method lookup_symbol` :204
          - `method find_callers` :225
          - `method find_callers_deep` :245
          - `method symbol_context` :289
          - `method list_definitions` :345
          - `method branch_diff` :361
          - `method list_all_nodes` :401
          - `method list_all_edges` :411
          - `method last_indexed_sha` :440
          - `method set_last_indexed_sha` :444
        - `constant NODE_COLS` :453
        - `function rows_to_nodes` :456
        - `function row_to_node` :464
        - `function collect_ids` :504
        - `function str_val` :518
        - `function i64_val` :527
        - `function bool_val` :537
        - `function kind_from_str` :548
        - `function edge_kind_from_str` :565
        - `function vis_str` :575
        - `function vis_from_str` :583
        - `function esc` :595
  - `pub folder gitcortex-indexer` :1
    - `pub folder src` :1
      - `pub folder parser` :1
        - `pub file python.rs` :1
          - `pub struct PythonParser` :15
            - `pub method new` :20
            - `method default` :28
            - `method extensions` :34
            - `method parse` :38
          - `struct FileVisitor` :72
            - `method new` :90
            - `method text` :132
            - `method span` :136
            - `method visibility` :144
            - `method qualified` :152
            - `method make_node` :160
            - `method collect_names` :189
            - `method visit_module` :233
            - `method visit_top_level` :241
            - `method visit_function` :269
            - `method visit_class` :316
            - `method maybe_visit_constant` :394
            - `method collect_imports` :420
            - `method collect_calls` :477
            - `method callee_name` :494
            - `method record_call` :505
            - `method fn_is_async` :530
            - `method collect_decorators` :538
            - `method decorator_name` :547
            - `method extract_param_types` :570
            - `method extract_return_type` :590
            - `method collect_type_names` :599
            - `method walk_type_names` :605
          - `function is_builtin_type` :625
          - `module tests` :684
            - `function parse` :690
            - `function parse_full` :702
            - `function parses_free_function` :726
            - `function parses_class_and_method` :736
            - `function detects_call_edges` :757
            - `function detects_base_class_implements` :765
            - `function detects_type_annotation_uses` :775
            - `function detects_decorator_uses` :786
            - `function detects_import_statement` :796
            - `function detects_from_import_statement` :810
            - `function module_node_is_emitted` :824
            - `function async_function_flagged` :832
        - `pub file go.rs` :1
          - `pub struct GoParser` :15
            - `pub method new` :20
            - `method default` :28
            - `method extensions` :34
            - `method parse` :38
          - `struct FileVisitor` :73
            - `method new` :91
            - `method text` :149
            - `method span` :153
            - `method visibility` :161
            - `method qualified` :174
            - `method make_node` :182
            - `method collect_names` :209
            - `method collect_type_decl_names` :234
            - `method visit_source_file` :253
            - `method visit_top_level` :261
            - `method visit_function` :271
            - `method visit_method` :291
            - `method receiver_type` :325
            - `method visit_type_decl` :350
            - `method visit_const_decl` :399
            - `method collect_imports` :417
            - `method record_import_spec` :445
            - `method collect_interface_assertions` :470
            - `method collect_candidate_type_names` :513
            - `method extract_fn_type_uses` :533
            - `method extract_struct_field_uses` :577
            - `method extract_interface_methods` :601
            - `method collect_type_idents` :630
            - `method walk_type_idents` :636
            - `method collect_calls` :655
            - `method callee_name` :672
            - `method record_call` :683
          - `function is_builtin_go_type` :707
          - `module tests` :738
            - `function parse` :744
            - `function parse_full` :754
            - `function parses_function` :767
            - `function parses_struct_and_method` :779
            - `function parses_interface` :800
            - `function go_visibility_is_uppercase` :809
            - `function detects_call_edges` :820
            - `function package_node_is_emitted` :828
            - `function detects_import_declaration` :837
            - `function detects_fn_type_uses` :851
            - `function detects_interface_assertion` :865
            - `function captures_interface_methods` :875
        - `pub file java.rs` :1
          - `pub struct JavaParser` :15
            - `pub method new` :20
            - `method default` :28
            - `method extensions` :34
            - `method parse` :38
          - `struct FileVisitor` :72
            - `method new` :90
            - `method text` :132
            - `method span` :136
            - `method visibility` :143
            - `method is_async` :161
            - `method qualified` :166
            - `method make_node` :174
            - `method collect_names` :201
            - `method visit_program` :223
            - `method visit_top_level` :231
            - `method visit_class` :241
            - `method visit_interface` :303
            - `method visit_enum` :345
            - `method visit_record` :383
            - `method visit_method` :411
            - `method collect_imports` :467
            - `method extract_annotation_uses` :497
            - `method extract_field_uses` :523
            - `method extract_simple_type` :532
            - `method collect_type_names` :550
            - `method walk_type_names` :556
            - `method collect_calls` :577
            - `method callee_name` :596
            - `method record_call` :616
          - `function is_builtin_java_type` :640
          - `module tests` :695
            - `function parse` :701
            - `function parse_full` :711
            - `function parses_class_and_method` :726
            - `function parses_interface` :739
            - `function parses_enum` :748
            - `function detects_extends_and_implements` :757
            - `function detects_type_annotation_uses` :768
            - `function detects_import_declaration` :779
            - `function module_node_is_emitted` :789
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
          - `struct FileVisitor` :114
            - `method new` :132
            - `method text` :173
            - `method span` :177
            - `method visibility` :184
            - `method is_async` :199
            - `method qualified` :205
            - `method make_node` :213
            - `method collect_names` :240
            - `method collect_names_from_var_decl` :272
            - `method unwrap_export` :295
            - `method visit_program` :318
            - `method visit_statement` :327
            - `method visit_function` :356
            - `method visit_class` :392
            - `method visit_method` :428
            - `method visit_interface` :455
            - `method visit_type_alias` :486
            - `method visit_enum` :500
            - `method visit_var_decl` :510
            - `method collect_imports` :565
            - `method extract_param_types` :629
            - `method extract_return_type_annotation` :645
            - `method extract_heritage` :656
            - `method collect_extends_names` :679
            - `method collect_implements_names` :699
            - `method extract_decorator_uses` :717
            - `method decorator_name` :729
            - `method collect_type_names` :751
            - `method walk_type_names` :757
            - `method collect_calls` :778
            - `method callee_name` :796
            - `method record_call` :807
          - `function is_builtin_ts_type` :831
          - `module tests` :879
            - `function parse_ts` :885
            - `function parse_ts_full` :897
            - `function parse_js` :911
            - `function parses_ts_function` :924
            - `function parses_ts_class_and_method` :933
            - `function parses_ts_interface` :954
            - `function parses_js_arrow_function` :962
            - `function detects_ts_call_edges` :973
            - `function detects_ts_extends_implements` :981
            - `function detects_ts_type_annotation_uses` :992
            - `function detects_ts_named_imports` :1003
            - `function module_node_is_emitted` :1017
        - `pub file mod.rs` :1
          - `pub module go` :8
          - `pub module java` :9
          - `pub module python` :10
          - `pub module rust` :11
          - `pub module typescript` :12
          - `pub struct ParseResult` :15
          - `pub trait LanguageParser` :34
          - `pub function parser_for_path` :45
  - `pub folder gitcortex-mcp` :1
    - `pub folder src` :1
      - `pub file main.rs` :1
        - `module cmd` :1
        - `module mcp` :2
        - `struct Cli` :11
        - `pub enum VizFormat` :17
        - `enum Commands` :25
        - `enum QueryCmd` :87
        - `function main` :115
      - `pub folder mcp` :1
        - `pub file tools.rs` :1
          - `pub struct LookupSymbolParams` :23
          - `pub struct FindCallersParams` :34
          - `pub struct ContextParams` :44
          - `pub struct ListDefinitionsParams` :52
          - `pub struct BranchDiffParams` :59
          - `pub struct DetectChangesParams` :65
          - `pub struct GitCortexServer` :75
            - `pub method new` :81
            - `method lookup_symbol` :98
            - `method find_callers` :136
            - `method context` :215
            - `method list_definitions` :256
            - `method branch_diff_graph` :289
            - `method detect_changes` :326
            - `method detect_impact` :424
            - `method generate_map` :460
          - `pub struct DetectImpactParams` :400
          - `pub struct GenerateMapParams` :408
          - `function run_git_diff` :511
          - `function parse_diff_hunks` :525
          - `function parse_hunk_header` :554
      - `pub folder cmd` :1
        - `pub file query.rs` :1
          - `pub function run` :9
          - `function repo_root` :82
        - `pub file init.rs` :1
          - `constant HOOK_NAMES` :15
          - `constant HOOK_SHEBANG` :22
          - `constant GH_WORKFLOW` :24
          - `constant CLAUDE_MD_SECTION` :58
          - `constant PRE_TOOL_USE_HOOK` :72
          - `constant SKILLS` :93
          - `constant SLASH_COMMANDS` :202
          - `pub function run` :227
          - `function install_hooks` :263
          - `function initial_index` :292
          - `function write_mcp_json` :315
          - `function write_slash_commands` :342
          - `function write_skills` :359
          - `function update_claude_md` :376
          - `function write_pre_tool_use_hook` :396
          - `function write_claude_settings` :415
          - `function add_gcx_hook_entry` :438
          - `function write_ci_workflow` :455
          - `function repo_root` :467
          - `function home_dir` :480
          - `function current_branch` :487

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


