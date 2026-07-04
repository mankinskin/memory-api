Representative baseline evidence from the first perf harness:

- move preflight on a reference-heavy parent↔submodule fixture: about 117 ms in e2e, and Criterion reported about 244-276 ms with sample-size 10.
- move execute on that representative fixture: about 11254 ms.
- move rollback on that representative fixture: about 11086 ms.
- move execute with missing tracked files/manual followups: about 8709 ms.
- representative health case wall time: about 162870 ms, with phase breakdown scan about 10193 ms, list about 37 ms, workflow build about 35 ms, collect_findings about 362 ms over 121 tickets, 118 edges, 265 findings.

These results indicate the immediate bottlenecks are not move preflight or raw health finding generation. The dominant costs are fixture/store scan work and move execute/rollback end-to-end file/index operations.