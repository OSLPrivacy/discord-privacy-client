# Task 5106 Discord composer references

`check.py` compares three independent inventories: the contract embedded in the
shipping Windows host, the dated live-carrier census produced before that
contract existed, and the durable reference index. It then strictly decodes
each lossless PNG, verifies the UIA ROI plus four-pixel seam geometry, the five
known-good colour floor, hashes, real signed-carrier provenance, and a distinct
reviewed-baseline record.

The Windows collector uses Windows PowerShell, UI Automation and
`System.Drawing.CopyFromScreen`. It never invokes Linux process/UI tools and
never persists its whole-window nonblankness capture.

Run the focused development proof with:

```text
python3 -m unittest -v test_check.py
```

Run the durable release check from the repository root with:

```text
python3 tools/task-5106-discord-composer-references/check.py
```
