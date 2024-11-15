# ANS-104 Bundle Parser CLI

This CLI tool is designed to parse ANS-104: Bundled Data v2.0 transactions on the Arweave blockchain. The primary objective is to handle potentially nested bundles with varying sizes and complexities, ensuring robust performance across a variety of edge cases.

---

## **Usage**

To run the CLI, use the following command:

`cargo run -- <tx_id> [OPTIONS]`

### **Positional Arguments**
- `<tx_id>`: The Arweave transaction ID to process.

### **Optional Parameters**
- `-o, --output`: Specifies the output file path. Defaults to `bundle`.

### **Example**

```
cargo run -- H95gGHbh3dbpCCLAk36sNHCOCgsZ1hy8IG9IEXDNl3o -o output
```

## **Goal**
The primary goal of this CLI is to parse and process bundled data transactions (ANS-104). The design focuses on handling:
- **Nested bundles**: Ensuring support for multiple levels of nested data.
- **Large numbers of entries**: Managing high entry counts with minimal memory overhead.
- **Huge data sizes**: Employing a streaming approach to process data that may exceed memory capacity.

### **Edge Cases Addressed**
1. **Large number of entries with small data sizes**:
   - Efficiently processes bundles with numerous small items.
2. **Huge data sizes**:
   - Handles streaming of large payloads without requiring full data to reside in memory.

### **Streaming Approach**
To handle large data sizes, a streaming approach is employed. This allows processing of massive bundles without requiring excessive memory, making the CLI capable of processing edge cases where data cannot fit in memory.

---

## **Specification Reference**

The implementation adheres to the ANS-104 specification:  
[ANS-104: Bundled Data v2.0](https://github.com/ArweaveTeam/arweave-standards/blob/master/ans/ANS-104.md)

---

## **Assumptions and Design Choices**

1. **Entries can fit in memory**:
   - The tool assumes that while data sizes may be large, the entries themselves can fit in memory. Extending support for larger entries would require saving entries to a database or disk.
   
2. **Streaming is sufficient for most cases**:
   - The tool processes bundles by streaming data. However, for scenarios where there are numerous entries with small data sizes, using range queries might be more efficient.

3. **Third-party Avro parsing library**:
   - For efficiency and simplicity, a third-party library is used for Avro decoding. While it works well for most cases, special error decoding could be implemented if necessary.

4. **No deep hash validation**:
   - The tool skips validation of DataItem signatures with deep hashing. This is assumed to be the responsibility of the uploader and end-users when verifying data integrity. The main reason was to avoid the need to sign the huge data size `DataItems`

5. **Error tolerance**:
   - Non-fatal errors (e.g., invalid entries) are logged, and the corresponding DataItem is skipped. Fatal errors (e.g., corrupt streams) terminate the process.

---

## **Performance Goals**

The CLI is designed to be:
- **Reasonably fast**: Good-enough for most real-world use cases, including large bundles with numerous small items.
- **Robust in edge cases**: Avoids breaking or panicking, even in edge cases involving large data sizes or deeply nested bundles (within memory constraints).


---

## **Limitations**
1. **Memory requirements for entries**:
   - Entries must fit in memory. Supporting larger entries would require disk-based storage or specialized processing.

2. **Deep hash validation**:
   - The tool does not perform full DataItem signature validation, leaving it to the data uploader or end-users. This was not to avoid implementing deep hash, but to avoid signing `DataItem`s with huge data

3. **Performance with massive bundles**:
   - While the tool is optimized for typical use cases, extreme scenarios with vast numbers of entries or extremely large payloads may still present challenges. 
   - The bundles with huge sized data object would need special treatment. Probably using multiple  http range queries in order to avoid reading data part at all.
   - On the other hand the entires with very large number of small entries would benefit more this kind of stream approach

4. **Full spec support**: The spec itself allows for super large number of entries 32byte number, with Nx64 number of entrie pairs. In order to support this efficiently we would probably need a more robust approach, with the cluster of instances.

---

## **Possible improvements**
- Add data storage as cache for entries, to support super large number of entries that can not fit the memory
- To support super large bundles, we would probably need to partition it and feed it to the more robust and large scale indexing cluster.



**Thank you!**
