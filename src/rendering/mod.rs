pub mod pdf;

use petgraph::visit::EdgeRef;

pub fn render_graph(graph: &pdf::Flow) {
    println!("digraph {{");

    // Print nodes
    for idx in graph.node_indices() {
        println!("    {} [ label = {} ]", idx.index(), graph[idx]);
    }

    // Print edges
    for edge in graph.edge_references() {
        println!(
            "    {} -> {} [ ]",
            edge.source().index(),
            edge.target().index()
        );
    }

    println!("}}");
}
