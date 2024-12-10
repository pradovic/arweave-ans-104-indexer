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

## **Specification Reference**

The implementation adheres to the ANS-104 specification:  
[ANS-104: Bundled Data v2.0](https://github.com/ArweaveTeam/arweave-standards/blob/master/ans/ANS-104.md

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

5. **Resumability**: The CLI expects to finish in one go. In order to support resumability, we would need to mark down what was indexed so far. It would probably be helpful to use additional data storage for this.

---
