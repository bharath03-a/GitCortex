// GitCortex Cypher export — branch main — 9 nodes, 5 edges
CREATE (`2598a175_2a19_5d0c_8fc0_5d7c56b0b483`:Symbol {name: '"x<script>`|"</script>λ', kind: 'function', file: 'tests/integration/fixtures/sample.rs', qualified_name: 'sample::function_0', start_line: 1});
CREATE (`3e49f44d_3c03_59c4_a88b_c5c9b3bcd140`:Symbol {name: 'function_1', kind: 'function', file: 'tests/integration/fixtures/sample.rs', qualified_name: 'sample::function_1', start_line: 2});
CREATE (`8fbc0cfc_0efd_574c_803e_6518a3384aaa`:Symbol {name: 'function_2', kind: 'function', file: 'tests/integration/fixtures/sample.rs', qualified_name: 'sample::function_2', start_line: 3});
CREATE (`c666d9fa_d801_59be_b8d9_79e491566390`:Symbol {name: 'function_3', kind: 'function', file: 'tests/integration/fixtures/sample.rs', qualified_name: 'sample::function_3', start_line: 4});
CREATE (`08db6c11_db36_5002_bc44_b802705909df`:Symbol {name: 'sample', kind: 'module', file: 'sample.rs', qualified_name: 'sample', start_line: 1});
CREATE (`4c9d203f_4915_591a_83e2_6b167d824259`:Symbol {name: 'Greeter', kind: 'trait', file: 'sample.rs', qualified_name: 'crate::Greeter', start_line: 1});
CREATE (`f42cf851_5de5_5cd6_8bdd_19293be765a0`:Symbol {name: 'Hello', kind: 'struct', file: 'sample.rs', qualified_name: 'crate::Hello', start_line: 5});
CREATE (`6ac25e01_c743_5012_9154_bbabd27cb257`:Symbol {name: 'greet', kind: 'method', file: 'sample.rs', qualified_name: 'crate::Hello::greet', start_line: 10});
CREATE (`15309f2e_98f6_51bf_97d1_93b5be5e5851`:Symbol {name: 'make_greeting', kind: 'function', file: 'sample.rs', qualified_name: 'crate::make_greeting', start_line: 15});
MATCH (a:Symbol), (b:Symbol) WHERE a.name IS NOT NULL AND id(a) = id(`2598a175_2a19_5d0c_8fc0_5d7c56b0b483`) AND id(b) = id(`3e49f44d_3c03_59c4_a88b_c5c9b3bcd140`) CREATE (a)-[:CALLS]->(b);
MATCH (a:Symbol), (b:Symbol) WHERE a.name IS NOT NULL AND id(a) = id(`3e49f44d_3c03_59c4_a88b_c5c9b3bcd140`) AND id(b) = id(`8fbc0cfc_0efd_574c_803e_6518a3384aaa`) CREATE (a)-[:CALLS]->(b);
MATCH (a:Symbol), (b:Symbol) WHERE a.name IS NOT NULL AND id(a) = id(`8fbc0cfc_0efd_574c_803e_6518a3384aaa`) AND id(b) = id(`c666d9fa_d801_59be_b8d9_79e491566390`) CREATE (a)-[:CALLS]->(b);
MATCH (a:Symbol), (b:Symbol) WHERE a.name IS NOT NULL AND id(a) = id(`f42cf851_5de5_5cd6_8bdd_19293be765a0`) AND id(b) = id(`4c9d203f_4915_591a_83e2_6b167d824259`) CREATE (a)-[:IMPLEMENTS]->(b);
MATCH (a:Symbol), (b:Symbol) WHERE a.name IS NOT NULL AND id(a) = id(`f42cf851_5de5_5cd6_8bdd_19293be765a0`) AND id(b) = id(`6ac25e01_c743_5012_9154_bbabd27cb257`) CREATE (a)-[:CONTAINS]->(b);
