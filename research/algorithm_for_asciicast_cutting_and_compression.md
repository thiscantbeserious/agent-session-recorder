# **Strategic algorithm planning and architecture design for high-performance processing of terminal session data**

## **1. Introduction and Problem Breakdown**

The development of robust algorithms for processing sequential data streams—especially those representing human-computer interaction in terminal environments—requires rigorous adherence to principles of structural decomposition, state management, and heuristic analysis. The need to “plan the algorithm properly” necessitates a shift away from ad-hoc scripting approaches toward a formal engineering discipline that considers both theoretical computer science and the pragmatic requirements of modern software architecture.  
This report articulates a comprehensive architectural design for a system intended to ingest, analyze, transform, and optimize terminal session recordings (specifically in the asciicast v3 format). The scope of this algorithmic design encompasses the entire data lifecycle: from the ingestion of raw, line-separated JSON streams (NDJSON), through complex transformation pipelines incorporating adaptive silence removal and heuristic pattern recognition (particularly for loading indicators or "spinners"), to the final serialization of the optimized artifact. By leveraging principles of stream processing, time series analysis, and finite automata theory, this report establishes a methodology that ensures data integrity, processing efficiency, and visual fidelity.

### **1.1 The Nature of Algorithmic Planning**

Designing algorithms is not merely writing code - it is the preceding phase of defining the logical steps, constraints, and data transformations required to efficiently solve a problem. As outlined in standard computer science pedagogy, the process moves from problem definition through analysis, design, and implementation to verification. For the specific task of processing terminal sessions, the "problem" is multifaceted: it involves temporal compression (removing silence) and cleaning up visual artifacts (spinner detection) without corrupting the playback experience. 

The initial phase requires a granular understanding of the input data. In this architecture, the input is the asciicast v3 format. Unlike its predecessors, v3 uses a newline delimited JSON (NDJSON) structure, where a header object is followed by a stream of event arrays. This structural change from a single JSON list (v2) to a stream (v3) dictates that the algorithm must be designed as a stateful stream processor rather than a document parser. The decision to adopt a streaming approach is not trivial; it is a direct response to the memory limitations encountered when processing logs that can contain gigabytes of data. An algorithm attempting to load the entire document into memory (DOM-based parsing) would inevitably fail with large session logs.

### **1.2 Phases of Algorithm Design**

To plan the algorithm “correctly”, we follow an established five-step model that ensures no critical requirements are overlooked:

1. **Problem definition**: A clear understanding of what needs to be solved is essential. Before an effective algorithm can be developed, a thorough understanding of the problem is vital. In the context of terminal sessions, this means finding the balance between data reduction (compression) and information preservation (readability).  
2. **Analysis and requirements gathering**: Breaking the problem down into its components. This includes identifying edge cases such as "split writes" (when a terminal command is distributed across multiple data packets) and analyzing the statistical properties of "silence" in user interactions.  
3. **Algorithm design**: The formulation of a step-by-step strategy. This is the core of this report, in which we define flowcharts (conceptually) and state machines.  
4. **Implementation strategy**: The translation of the design into actual code, with a focus on choosing the right tools (e.g., Rust, serde, tokio) to practically implement the theoretical requirements.  
5. **Testing and optimization**: Validating the solution and improving its performance.

### **1.3 Limitations and boundary conditions**

Identifying constraints is crucial for a robust design. Algorithms do not exist in a vacuum; they are limited by hardware resources and data complexity.

* **Storage complexity**: Terminal logs can become extremely large. A naive approach that stores all events in a single `Vec<Event>` is unacceptable. The algorithm must operate with `O(1)` memory relative to the file size, which means it must process data line by line or in small chunks.  
* **Time complexity**: Since processing is often part of a pipeline (e.g., live streaming or CI/CD logs), throughput must be maximized. `O(N)` is the target, where N is the number of events. Quadratic complexity `O(N^2)` – for example, by repeatedly scanning the entire stream for each spinner – must be avoided.  
* **Data integrity in Unicode**: Output events ("o") can split multi-byte Unicode characters across separate events. An algorithm that blindly cuts a stream in the middle of a UTF-8 sequence will cause invalid JSON and rendering errors.  
* **Heuristic ambiguity**: A spinner character (e.g., | or \-) is often identical to regular characters in shell commands or source code. The algorithm must use context (repetition, backspaces, carriage returns) to distinguish a spinner from code.

## **2. Analysis of data structures and schema definition**

Precise planning requires a deep understanding of the input data. The asciicast v3 format represents an evolution from v2, with specific implications for algorithm design. It is no longer just a file, but a stream of events, enabling real-time data processing but also necessitating robust synchronization.

### **2.1 The Header Specification**

The header is the first object in the stream and defines the global context of the session. It is a JSON object containing metadata. For the algorithm, parsing this header is the initialization step (bootstrap) that configures all subsequent state machines.

| Field | Type | Necessary | Description & Algorithmic Relevance |
| :---- | :---- | :---- | :---- |
| version | integer | And | Must have the value 3\. The algorithm must validate this and abort if an incompatible version (e.g., 2\) is detected. |
| width | integer | Yes (in the term) | The width of the terminal in columns. This is crucial for algorithms that need to emulate the visual state (e.g., line break handling). |
| height | integer | Yes (in the term) | The terminal's height in lines. Important for paging and scrolling logic. |
| timestamp | integer | No | Unix timestamp of the recording start. Serves as a reference point for absolute time calculations, if required. |
| idle\_time\_limit | float | No | A suggested value for players to limit inactivity. The silence removal algorithm can use this value as a default parameter, but it should also be overridable. |
| env | object | No | Environment variables such as SHELL or TERM. Useful for context analysis, but often irrelevant for core processing. |

A robust implementation must define a header struct (e.g., in Rust using `serde::Deserialize`) that strictly types these fields and handles optional fields using `Option<T>`. An error in the header (e.g., a missing `version`) must lead to immediate termination (fail-fast principle).

### **2.2 The Event Stream**

Following the header is the actual content: a sequence of events. Each event is encoded in asciicast v3 as a 3-element array (tuple): `[interval, code, data]`. This compact representation significantly reduces the overhead compared to JSON objects with keys (`{"time":..., "type":...}`), but requires a position-based parser.

#### **2.2.1 The Interval (Interval)**

The first element is a float that represents the time interval in seconds since the*previous*indicates event.

* **Implikation**: This is a*relative* Time measurement (delta coding). To determine the absolute time of an event, the algorithm must include an accumulator (current\_time \+= interval).  
* **Advantage**: Removing silence is trivial. It's not necessary to recalculate all subsequent timestamps (as with absolute timestamps in v2), but simply to trim the interval value of the current event. This makes operations like cut or trim local and efficient.  
* **precision**: The values ​​can be very small (e.g. 0.0001 for fast outputs) or very large (10.0 for inactivity).

#### **2.2.2 The Event Code**

The second element is a string that determines the type of event. The algorithm must implement match logic (switch-case) here to handle events differently.

- **"o" (Output)**: Data written to stdout. This is the primary content. The spinner detection algorithm focuses almost exclusively on this type.  
- **"i" (Input)**: Data sent to stdin (keyboard input). By default, this is often not recorded, but can be relevant for password filtering or security analysis.  
- **"m" (Marker)**: This event is used for navigation or annotation (e.g., chapter markers). The algorithm should preserve these events because they have semantic value, but they do not directly affect the visual presentation.  
- **"r" (Resize)**: Signals a change in terminal size (`SIGWINCH`). The data field has the format "WxH" (e.g., "100x40"). An emulator algorithm must resize its internal canvas buffer upon this event.  
- **"x" (Exit)**: Indicates the exit status of the process (e.g., "0" for success). This usually marks the end of the stream.

#### **2.2.3 The data (payload)**

The third element contains the actual user data.

- For "o" and "i", this is a UTF-8 string that often contains ANSI escape sequences. Non-printable characters must be escaped according to JSON RFC (Section 2.5) (e.g., \u001b for escape).  
- **Challenge**: The string can *one*. The algorithm must include a Unicode code point. It must ensure that when cutting or parsing strings (e.g., for spinner detection), it respects grapheme clusters and not bytes. In Rust, this means using iterators over `chars()` or external crates like `unicode-segmentation` instead of simple byte indexing.

## **3\. Stream Processing Architecture**

The choice of architecture determines the performance and scalability of the algorithm. Given the potential size of the data streams, a**Streaming pipeline architecture** (Source-Transform-Sink) mandatorisch.

### **3.1 The Pipeline Model**

The system is designed as a chain of processors that pass data through. This model, often known as "pipes and filters," allows for a clear separation of responsibilities.

1. **Source (What)**: A buffered reader that consumes the raw byte stream. It is responsible for the low-level parsing of NDJSON. In Rust, `serde_json::Deserializer::from_reader(reader).into_iter()` is a suitable option, providing an iterator over `Result<Event>`. This is memory-efficient because only the current event is held in memory.  
2. **Transformer Chain (processing chain)**: A series of modules that manipulate the event stream.  
   - *Silence Remover*: A filter that modifies the interval values.  
   - *Spinner Detector*: A complex buffer that temporarily holds back events to identify patterns and then either modifies (summarizes) them or forwards them unchanged.  
   - *Normalizer*: An optional step that, for example, repairs invalid UTF-8 sequences or cleans up ANSI codes.  
3. **Sink (Sink)**: A buffered writer that serializes the (possibly modified) events back into NDJSON format and writes them to the hard drive or a network socket.

### **3.2 Implementing `NDJSON` Parsing in Rust**

Parsing "Newline Delimited JSON" in strongly typed languages ​​like Rust is challenging if you want maximum performance. serde\_json offers specialized tools for this.  
To keep memory consumption to a minimum, a specific struct that reflects the scheme should not be used, but rather a generic value type.  

```rust
  // Pseudocode / Rust structure for the event
  struct AsciicastEvent(f64, String, Value);

  // We are temporarily using Value for the third field,
  // because depending on the code it could be a string or (theoretically) an object,
  // although v3 specifies that it is usually a string.
```
For extremely large files, where even the overhead of string allocations for each event is too high, techniques like zero-copy deserialization (using `Cow<str>`) can be considered. However, this is complex with a stream reader because it overwrites the reader's buffer. A pragmatic approach is to use DeserializeSeed to enable stateful deserialization if context (such as the header) is needed to interpret the events.

### **3.3 Buffer Strategies and Latency**

A critical aspect of stream processing is buffer management.

- **Input buffer**: `BufReader` (e.g., with an 8KB buffer) drastically reduces the number of system calls (`read` syscalls).                                                                                              - **Processing buffer**: For spinner detection (see section 5), a lookahead buffer or sliding window is required. The algorithm cannot decide whether a `|` character is a spinner or a pipe without the *following* events (e.g., a `\b` and a `/`) being visible. This buffer introduces latency into the pipeline, which must be taken into account for live streaming.
- **Output buffer**: `BufWriter` collects small write operations (e.g., single event lines) to perform block writes to the hard drive, maximizing I/O efficiency.                                                                                  
## **4. Heuristic Algorithms: Silence Removal**

The first major transformation in the algorithm is time compression. Terminal sessions often contain long pauses while the user thinks or reads something in another window. This "silence" makes playback tedious and inefficient.

### **4.1 Theoretical Foundations and Mathematical Model**

The problem of silence removal in event streams is analogous to "adaptive silence removal" in audio signal processing, but on the time axis instead of the amplitude. In an audio signal, a segment is classified as silence if the amplitude is  below a threshold (`A < A_threshold`). In an asciicast stream, "silence" is defined as an interval that exceeds a certain duration (`Δt > T_max`).                                                                                                 
The transformation function `f(Δt_i)` for the interval `Δt_i` of an event `E_i` can be formally defined as:                                                                                                               

> Where `T_{limit}` is the configured threshold (e.g., `idle_time_limit` from the header or a user parameter).  

### **4.2 The interval truncation algorithm**

This algorithm is stateless with respect to the content, but it changes the global time structure of the session.  

**Algorithmus 4.1: Stream Interval Clamping**
  1. **Initialization**: Read `T_{limit}` from configuration or header.
  2. **Loop**: For each event `E_{in} = [t_{delta}, code, data]` in the input stream:
     a. **Test**: Is `t_{delta} > T_{limit}`?
     b. **Transformation**:
        - If yes: Set `t_{new} = T_{limit}`.
        - If no: Set `t_{new} = t_{delta}`.
     c. **Emission**: Write `E_{out} = [t_{new}, code, data]` to the output stream.
  3. **End**.                                                                                                                                                                                                                                        
                

This approach is simple but effective. It preserves the relative rhythm of typing (short intervals are retained) but eliminates excessive waiting times. A more advanced variant could use non-linear compression (e.g., logarithmic scaling of long pauses), but simple clamping is the industry standard for players like asciinema.

### **4.3 Adaptive Thresholds**

In more complex scenarios, a fixed threshold may be insufficient. For example, a 5-second pause after a complex command (compilation) might be helpful for the viewer to read the result, while 5 seconds in the middle of typing a word would be disruptive. A "smart silence" algorithm could analyze the context:

* If the previous event contained a "Newline" (\n) (end of a command), temporarily increment `T_{limit}`.  
* If the previous event was a letter (in the middle of a word), use a strict `T_{limit}`. This requires a lookbehind buffer from an event to evaluate the context.

## **5. Heuristic Algorithms: Visual Artifact Detection (Spinner Detection)**

The detection and handling of loading animations ("spinners") is the most complex component of the planned algorithm. Spinners are visual indicators of background activity, implemented in text form by rapidly overwriting characters.

### **5.1 Taxonomy of CLI loading indicators**

To identify spinners, we need to know what to look for. The libraries indicatif (Rust) and cli-spinners (JavaScript) define the de facto standard for these patterns. A spinner consists of a sequence of frames (individual images) and an interval.  
**Table 1: Common spinner patterns and their characteristics**

| Name | Intervall (ms) | Frames (sequence) | Unicode range | Those |
| :---- | :---- | :---- | :---- | :---- |
| **Dots** | 80 | ⠋, ⠙, ⠹, ⠸, ⠼, ⠴, ⠦, ⠧, ⠇, ⠏ | Braille Patterns (U+2800 \- U+28FF) |  |
| **Line** | 130 | \-, \\, \` | , /\` | Basic Latin / ASCII |
| **Balloon** | 140 | ., o, O, @, \* | Basic Latin |  |
| **Arrow** | 100 | ←, ↖, ↑, ↗, →, ↘, ↓, ↙ | Arrows (U+2190 \- U+21FF) |  |
| **Block** | 100 | ▖, ▘, ▝, ▗ | Block Elements (U+2580 \- U+259F) |  |

The challenge lies in the fact that many of these characters (especially ASCII characters like - or |) are common in normal text. A | is a pipe in the shell. A . is a punctuation mark. Therefore, the heuristic must consider more than just the character itself.

### **5.2 State machine for pattern recognition**

The algorithm must be modeled as a `Finite State Machine (FSM)` in order to track the context over time.  

**Conditions:**

1. **IDLE (idle)** The default state. We forward events.  
2. **CANDIDATE (Candidate)** We saw a sign that the*first frame*`, This could be the event of a known spinner (e.g., `⠋`). We buffer this event instead of outputting it.  
3. **VERIFIING** We are observing the following events.  
   * Are we expecting a cursor control character? (Spinners often use `\b` (Backspace) or `\r` (Carriage Return) to reset the cursor).  
   * Do we expect the *next frame* the sequence (e.g. `⠙` after `⠋`)?  
4. **MATCHED (Recognized)** The pattern has been confirmed (e.g., 3 frames in the correct order with matching intervals). We process the entire buffer as a "spinner sequence".  
5. **REJECT (Rejection)** The sequence was broken (e.g., `⠋` followed by hello). We flush the buffer (output all held events unchanged) and return to IDLE.

### **5.3 The "Sliding Window" and Lookahead**

Since spinners are defined over time, we need a "sliding window" buffer. Ideally, the size of this window (W) should be at least as large as the cycle length of the longest spinner to be detected (e.g., 10 frames for "dots" + control characters = approximately 20 events).  

**Algorithm 5.1: Heuristic Spinner Detection**

  1. **Input**: Event stream.                                                                                                                                                                                            
  2. **Buffer**: `LookaheadBuffer` (capacity `N`).
  3. **Logic**:
     a. Read event `E`.
     b. Is `E.code == "o"`?
        - No: Empty buffer (output), print `E`. Reset FSM.
        - Yes: Analyze `E.data`.
     c. **Analyze**:
        - Clean `E.data` of ANSI codes (see 5.4).
        - Compare the cleaned character with the spinner database.
        - Is it frame `F_0` of a spinner `S`? -> Go to state `CANDIDATE`.
        - Is it frame `F_{i+1}` of the current candidate `S`? -> Stay in `VERIFYING`.
  4. **Decision**:
     - If buffer is full or timeout occurs: Decide (`MATCHED` or `REJECT`).
     - If `MATCHED`: Replace the sequence with a simplified event (e.g., only the last frame or a metadata marker). 

### **5.4 The problem of "split writes" and ANSI codes**

A technical detail that is often overlooked is the fragmentation of write operations. A program might use `printf("\x1b")`. Before the heuristic is applied, fragments must be assembled into logical units (graphemes). A coalescing buffer can hold incomplete escape sequences until they are completed.

### **5.5 Unicode and Grapheme Clusters**

As mentioned in the snippets, modern spinners often use Unicode. A character like `⠋` takes up more than one byte in memory (3 bytes in UTF-8). If the data stream is split mid-character, invalid sequences are created. The algorithm must ensure that it never slices bytes, but always operates at the char (Unicode Scalar Value) level or, even better, at the grapheme cluster level (for emojis and combination characters). In Rust, the standard library `str::chars()` and the crate unicode-segmentation provide the necessary tools.

## **6. Implementation strategy and verification**

Implementing this plan requires choosing suitable technologies. Rust is the optimal choice due to its memory reliability, performance, and excellent support for WebAssembly (for potential browser players).

### **6.1 Rust-Toolchain**

Based on the analysis results, we recommend the following crates:

* **header & header_json**: For parsing and serialization. The StreamDeserializer structure is essential for memory optimization.  
* **such**: As an asynchronous runtime to perform non-blocking I/O operations, which is particularly important for pipes (cat log.cast | processor | player).  
* **indicator**: Not only does it serve as a source of inspiration for spinner data, but it can also be used to display progress during processing (meta-level).  
* **spinners-rs**: Provides ready-made HashMaps/Enums of the most common Spinner frames, eliminating the need for manual database maintenance.  
* **console / vt100**: For parsing ANSI codes and emulating the terminal state to handle split writes.

### **6.2 Test strategy**

Heuristic algorithms are prone to errors. A deterministic testing strategy is necessary.

1. **Synthetic Tests** Generation of NDJSON streams containing artificial spinners (perfect sequences). The algorithm must demonstrate a 100% detection rate.  
2. **Fuzzing**: Injecting random data and corrupt ANSI sequences to ensure that the parser does not crash (panic safety) and that the state recovers (self-healing).  
3. **Visual Regression Tests**: Using a headless player to take screenshots of the original and processed sessions. The visual difference should be minimal (except in the timestamps).  
4. **Randfall-Tests**: Specific tests for buffer size limits and for Unicode segmentation problems.

### **6.3 Performance Targets**

The target system should be capable of processing protocols at a speed of >100 MB/s. Since most operations are CPU-bound (JSON parsing, regex/string matching), parallelization is an option.

* **Thread 1**: JSON Reader & Deserializer.  
* **Thread 2**: Heuristics Engine (Silent & Spinner).  
* **Thread 3:** Serializer & Writer. Communication takes place via bounded channels (mpsc) to handle backpressure and keep memory consumption constant.

## **7. Summary and Outlook**

Designing an algorithm for processing terminal sessions is an exercise in precision. It's not enough to simply "delete pauses." You have to understand the semantics of those pauses. It's not enough to "remove spinners." You have to maintain the visual integrity of the stream.  
This report has shown that by combining:

1. One **Streaming architecture** (NDJSON, Iterator),  
2. A robust **State machines** for pattern recognition,  
3. A mathematically sound **Time compression**,  
4. And the strict adherence to **Data standards** (Unicode, ANSI),

A system can be created that transforms raw log data into optimized, visually appealing representations. The presented design patterns form the foundation for an implementation that is both performant and maintainable, meeting the high demands of professional software development. The next steps would be to prototyp the heuristic engine in Rust and validate it against a corpus of real asciicast files.

#### **Sources**

  1. What are the 5 steps of an algorithm? - Design Gurus, https://www.designgurus.io/answers/detail/what-are-the-5-steps-of-an-algorithm
  2. What is an Algorithm | Introduction to Algorithms - GeeksforGeeks, https://www.geeksforgeeks.org/dsa/introduction-to-algorithms/
  3. The Four Major Stages of Algorithm Analysis and Design - PremiumCoding, https://premiumcoding.com/major-stages-algorithm-analysis-design/
  4. asciicast v3 - asciinema docs, https://docs.asciinema.org/manual/asciicast/v3/
  5. Posts - asciinema blog, https://blog.asciinema.org/post/
  6. Efficiently Processing Gigantic JSON Objects with Rust, Serde, and Tokio - ixpantia, https://www.ixpantia.com/en/blog/json-with-rust-serde-tokio
  7. Memory-efficient parsing of a large amount of JSON data : r/rust - Reddit, https://www.reddit.com/r/rust/comments/83a7iv/memoryefficient_parsing_of_a_large_amount_of_json/
  8. Algorithm DIY: How to Build Your Own Algorithm in 9 Steps - Klipfolio, https://www.klipfolio.com/blog/algorithm-in-six-steps
  9. Strategies for Handling Algorithm Edge Cases: Mastering the Art of Robust Code, https://algocademy.com/blog/strategies-for-handling-algorithm-edge-cases-mastering-the-art-of-robust-code/
  10. Command line spinners: the magic tale of modern typewriters and terminal movies, https://odino.org/command-line-spinners-the-amazing-tale-of-modern-typewriters-and-digital-movies/
  11. kojix2/spinner2: A terminal spinner for tasks that have a non-deterministic time frame - GitHub, https://github.com/kojix2/spinner2
  12. Rationale for Timestamps in .cast format - Help - asciinema forum, https://discourse.asciinema.org/t/rationale-for-timestamps-in-cast-format/657
  13. indicatif/src/lib.rs at main - GitHub, https://github.com/console-rs/indicatif/blob/master/src/lib.rs
  14. StreamDeserializer in serde_json - Rust - Docs.rs, https://docs.rs/serde_json/latest/serde_json/struct.StreamDeserializer.html
  15. serde_json::StreamDeserializer - Rust - Starry Network, https://starry-network.github.io/starry_node/serde_json/struct.StreamDeserializer.html
  16. serde-rs/json: Strongly typed JSON library for Rust - GitHub, https://github.com/serde-rs/json
  17. Parse arbitrarily large JSON array in Rust - Stack Overflow, https://stackoverflow.com/questions/76470303/parse-arbitrarily-large-json-array-in-rust
  18. Adaptive noise cancelling - Wikipedia, https://en.wikipedia.org/wiki/Adaptive_noise_cancelling
  19. How to Remove Silence from an Audio using Python | by Onkar Patil - Medium, https://onkar-patil.medium.com/how-to-remove-silence-from-an-audio-using-python-50fd2c00557d
  20. Mastering State Machines with Activity Diagrams for System & Software Design - YouTube, https://www.youtube.com/watch?v=ZURvW0761A8
  21. Adaptive Noise Cancelling: Principles and Applications - Information Systems Laboratory, https://www-isl.stanford.edu/~widrow/papers/j1975adaptivenoise.pdf
  22. Progress Bars in Python: A Complete Guide with Examples - DataCamp, https://www.datacamp.com/tutorial/progress-bars-in-python
  23. List all available spinners — list_spinners - cli, https://cli.r-lib.org/reference/list_spinners.html
  24. A Rust library for displaying terminal spinners - GitHub, https://github.com/mainrs/terminal-spinners-rs
  25. Block Elements - Wikipedia, https://en.wikipedia.org/wiki/Block_Elements
  26. Visualisation of state machines using the Sugiyama framework - Chalmers Publication Library, https://publications.lib.chalmers.se/records/fulltext/161388.pdf
  27. State Machine Diagrams | Unified Modeling Language (UML) - GeeksforGeeks, https://www.geeksforgeeks.org/system-design/unified-modeling-language-uml-state-diagrams/
  28. What is the Sliding Window Method in Time Series Analysis? - Lazy Programmer, https://lazyprogrammer.me/what-is-the-sliding-window-method-in-time-series-analysis/
  29. Sliding Window Technique — reduce the complexity of your algorithm | by Data Overload, https://medium.com/@data-overload/sliding-window-technique-reduce-the-complexity-of-your-algorithm-5badb2cf432f
  30. Cooler ASCII Spinners? [closed] - Stack Overflow, https://stackoverflow.com/questions/2685435/cooler-ascii-spinners
  31. spinners-rs - crates.io: Rust Package Registry, https://crates.io/crates/spinners-rs
  32. Provide a way for StreamDeserializer to stream an array of values · Issue #404 · serde-rs/json - GitHub, https://github.com/serde-rs/json/issues/404           
