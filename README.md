# Concurrent Flow Language

A language for describing concurrent execution flows with support for parallel, sequential, and explicit dependency operations.

## Quick Start

```bash
# Install dependencies
cargo build --release

# Generate a simple graph
cargo run -- '$s0,s1,s2$' output.pdf

# View all examples
bash generate_examples.sh
```

## Features

* ✅ **Simple and expressive syntax** for defining concurrent flows
* ✅ **Sequential execution** with `[...]`
* ✅ **Parallel execution** with `{...}`
* ✅ **Explicit dependencies** with `#{...}`
* ✅ **Terminal nodes** with `!` to control connection propagation
* ✅ **Automatic validation** of circular and missing dependencies
* ✅ **PDF generation** with flow graphs
* ✅ **Unlimited nested structures**
* 🚧 **Conversion to/from Fork/Join** (⚠️ partial implemented)
* 🚧 **Conversion to/from Parbegin/Parend** (planned)
* 🚧 **WebAssembly compilation** (planned)
* 🚧 **Visual block editor** (planned)

## What is this?

This project is an **intermediate language** for representing concurrent execution flows. It allows you to:

1. **Describe** concurrent programs in a simple and visual way
2. **Automatically validate** dependency correctness
3. **Visualize** the flow as a PDF graph

**Future goal:** Serve as an intermediate language between different notations (Fork/Join, Parbegin/Parend) and enable the creation of web-based visual editors using WebAssembly.

## Syntax

### Basic Program

Every program must be enclosed between `$` delimiters:

```
$<nodes>$
```

### Atomic Nodes

An atomic node represents an individual task:

```
s0
```

### Sequences

Nodes inside square brackets `[]` execute **sequentially** (one after another):

```
[s0, s1, s2]
```

### Parallelism

Nodes inside curly braces `{}` execute **in parallel** (simultaneously):

```
{s0, s1, s2}
```

### Dependencies

A node can have explicit dependencies using `#{}`:

```
s1#{s0}
```

This means: "s1 depends on s0" (s0 must complete before s1 can start).

Multiple dependencies:

```
s3#{s0, s1, s2}
```

### Terminal Nodes

A node marked with `!` is **terminal** — it does not propagate connections to subsequent nodes:

```
s6!
```

This is useful when a node should not be considered a prerequisite for later nodes in the chain.

## Examples

### Example 1: Simple Sequence

```
$s0, s1, s2$
```

Executes s0, then s1, then s2.
See: [examples/sequence.graph](examples/sequence.graph) | [PDF](examples/output/sequence.pdf)

### Example 2: Parallel Execution

```
$s0, {s1, s2, s3}, s4$
```

Executes s0, then s1/s2/s3 in parallel, then s4.
See: [examples/parallel.graph](examples/parallel.graph) | [PDF](examples/output/parallel.pdf)

### Example 3: With Dependencies

```
$s0, s1, s2#{s0, s1}, s3$
```

s2 must wait for both s0 and s1 to finish.
See: [examples/dependencies.graph](examples/dependencies.graph) | [PDF](examples/output/dependencies.pdf)

### Example 4: Terminal Nodes

```
$s0, s1, {[s2, s5#{s3}, s8#{s6}], [s3, s6!], [s4, s7, s9#{s6}]}, sa#{s8, s9}$
```

s6 is terminal (`s6!`), so it does not propagate to `sa`. Only s8 and s9 connect to sa.
See: [examples/terminal.graph](examples/terminal.graph) | [PDF](examples/output/terminal.pdf)

### Example 5: Complex (Nested Structures)

```
$s0,{[s1,s2#{s1}],[s3,{s4,s5}]},s6,{s7,s8,s9},s10#{s6,s7,s8,s9}$
```

Combines sequences inside parallels, parallels inside sequences, and multiple dependencies.
See: [examples/complex.graph](examples/complex.graph) | [PDF](examples/output/complex.pdf)

## Syntax Table

| Syntax      | Description                    | Example           |
| ----------- | ------------------------------ | ----------------- |
| `$...$`     | Program delimiters             | `$s0, s1$`        |
| `s0`        | Atomic node (task)             | `s0`              |
| `[a, b, c]` | Sequence (strict order)        | `[s0, s1, s2]`    |
| `{a, b, c}` | Parallel (simultaneous)        | `{s0, s1, s2}`    |
| `s1#{s0}`   | Explicit dependencies          | `s3#{s0, s1, s2}` |
| `s6!`       | Terminal node (no propagation) | `[s5, s6!]`       |

## Visual Concepts

### Sequence vs Parallel

```
Sequence: [s0, s1, s2]
  s0 → s1 → s2

Parallel: {s0, s1, s2}
       ┌→ s0 ─┐
  start → s1 → end
       └→ s2 ─┘
```

### Dependencies

```
s2#{s0, s1}

  s0 ─┐
      ├→ s2
  s1 ─┘
```

### Terminal Nodes

```
[s0, s1!], s2

Without !:  s0 → s1 → s2  (s1 connects to s2)
With !:     s0 → s1!      (s1 does NOT connect to s2)
            s2
```

## Validation

The parser automatically detects:

* ❌ **Circular dependencies**: `$s0#{s1}, s1#{s0}$`

  * See: [examples/error_circular.graph](examples/error_circular.graph)
* ❌ **Missing dependencies**: `$s0#{s_missing}$`

  * See: [examples/error_missing.graph](examples/error_missing.graph)

## Usage

### Generate PDF from expression

```bash
cargo run -- '$s0, s1, {s2, s3}, s4$' output.pdf
```

### Generate PDF from file

```bash
cargo run -- "$(cat my_graph.graph)" output.pdf
```

## Roadmap

* [ ] Conversion to/from Fork/Join notation
* [ ] Conversion to/from Parbegin/Parend notation
* [ ] WebAssembly compilation for web interface
* [ ] Visual block editor with PDF export

## Project Structure

```
concurrent/
├── grammar/          # Grammar definition (Pest)
├── src/
│   ├── parser/      # Parser and data structures
│   ├── rendering/   # Graph and PDF generation
│   └── validator.rs # Dependency validation
└── examples/        # Graph examples
```
