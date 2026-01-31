# Concurrent | Lenguaje de Flujos Concurrentes

**[English](#english)** | **[Español](#español)**

---

## English

### Description
A minimalist language for describing concurrent execution flows with automatic validation and PDF visualization.

### Installation
```bash
cargo build --release
```

### Usage
```bash
# Generate PDF from expression
cargo run -- '$s0,{s1,s2},s3$' output.pdf
```

### Syntax

| Syntax | Description | Example |
|--------|-------------|---------|
| `$...$` | Program delimiters | `$s0,s1$` |
| `[a,b]` | Sequential execution | `[s0,s1,s2]` |
| `{a,b}` | Parallel execution | `{s0,s1,s2}` |
| `a#{b}` | Explicit dependency | `s2#{s0,s1}` |
| `a!` | Terminal node | `s5!` |

### Examples

**Sequential:**
```
$s0,s1,s2$
```

**Parallel:**
```
$s0,{s1,s2,s3},s4$
```

**With dependencies:**
```
$s0,s1,s2#{s0,s1},s3$
```

**Complex nested:**
```
$s0,{[s1,s2#{s1}],[s3,{s4,s5}]},s6$
```

### Features
✅ Sequential & parallel execution  
✅ Explicit dependencies  
✅ Circular dependency detection  
✅ PDF graph generation  
✅ Unlimited nesting  
🚧 Fork/Join conversion (partial)  
🚧 Parbegin/Parend conversion (partial)  

### Examples
See [examples/](examples/) for `.graph` files and generated PDFs.

---

## Español

### Descripción
Un lenguaje minimalista para describir flujos de ejecución concurrentes con validación automática y visualización en PDF.

### Instalación
```bash
cargo build --release
```

### Uso
```bash
# Generar PDF desde una expresión
cargo run -- '$s0,{s1,s2},s3$' salida.pdf
```

### Sintaxis

| Sintaxis | Descripción | Ejemplo |
|----------|-------------|---------|
| `$...$` | Delimitadores del programa | `$s0,s1$` |
| `[a,b]` | Ejecución secuencial | `[s0,s1,s2]` |
| `{a,b}` | Ejecución paralela | `{s0,s1,s2}` |
| `a#{b}` | Dependencia explícita | `s2#{s0,s1}` |
| `a!` | Nodo terminal | `s5!` |

### Ejemplos

**Secuencial:**
```
$s0,s1,s2$
```

**Paralelo:**
```
$s0,{s1,s2,s3},s4$
```

**Con dependencias:**
```
$s0,s1,s2#{s0,s1},s3$
```

**Anidamiento complejo:**
```
$s0,{[s1,s2#{s1}],[s3,{s4,s5}]},s6$
```

### Características
✅ Ejecución secuencial y paralela  
✅ Dependencias explícitas  
✅ Detección de dependencias circulares  
✅ Generación de grafos en PDF  
✅ Anidamiento ilimitado  
🚧 Conversión Fork/Join (parcial)  
🚧 Conversión Parbegin/Parend (parcial)  

### Ejemplos
Ver [examples/](examples/) para archivos `.graph` y PDFs generados.
