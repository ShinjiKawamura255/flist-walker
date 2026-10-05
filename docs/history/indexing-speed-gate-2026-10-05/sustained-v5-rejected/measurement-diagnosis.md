# Rejected sustained-policy reference comparison

The original v5 source is `83aa173deeb16308c838606313131843247f566c`, fixed reference `5ab325136bbca4d5a94d541b708f2e07c1eb1255`, run [37330978548 attempt1](https://github.com/ShinjiKawamura255/flist-walker/actions/runs/37330978548). The successful inactive workflow is not accepted numeric calibration. [Original admission](admission-summary.json), [149-file archive membership](evidence-manifest.json) and [original-source CLI replay](roundtrip-replay.json) retain its rejection.

S1-ignore/FileList/control has the same t2/t3 values. All seven points remain, in pair order:

| Role | All seven values, ms | Median, ms |
| --- | --- | --- |
| R1 | 514.428, 339.764, 403.411, 339.831, 371.683, 290.949, 484.576 | 371.683 |
| Candidate | 274.569, 387.110, 322.791, 386.598, 258.311, 257.853, 258.589 | 274.569 |
| R2 | 322.123, 290.444, 290.933, 305.995, 322.159, 274.643, 289.882 | 290.933 |

The symmetric reference-median drift is1.2775544302679285, above1.25, so both mandatory endpoint comparisons are indeterminate. All68 candidate medians are within their1.50 limits. F1 and stable proposals pass; all68 maxima remain diagnostics, including f1 candidate maximum ratio1.611466746282174. No candidate regression, freeze, later session or enforcement activation is inferred.

Independent read-only diagnosis inspected all three matched raw logs and the unchanged driver. R1/R2 median samples have24/19frames, producer data completion204.705/202.185ms, Full wait94.422/94.275ms and79,809/89,765 ingested entries at first terminal observation. Both then drain2048 entries/frame. The visible difference lies in GUI intake before terminal and remaining drain. All42 matched rows have zero timed fixture scans; the target control traces have no truncation, filter cursor or result debt, and frame-start gaps are about16.1ms. Walker medians drift by about1%; FileList condition t2/t3 drift1.144/1.114. Uniform host drift, CPU/I/O cause and observer defect are not established.

Each arm creates a new Driver/App/settings/worker runtime, settles setup, verifies full oracles and search-worker quiescence, then explicitly drops the driver before returning its row. Registered workers, tab shutdown drain and reclaimer participate in bounded App shutdown; settings have a separate strict physical barrier. Original matched logs contain no shutdown timeout, panic or teardown failure. Worker/request carryover is not established. Fixture reuse, process allocator, OS cache and CPU state remain shared; neither independence nor a fixed order bias is proved. R1 A-first controls are slower, candidate order differences reverse, and R2 differences are small. Warmup is present, but its convergence is unmeasured.

A limited prospective candidate is21 matched pairs while keeping f1=21/stable=7, every sample,1.50/1.25,MAX diagnostics,100k,16ms,warmup,arm order,endpoints,runtime and all admission/budgets. More observations could test median representativeness over the broad seven-point distribution. They cannot fix systematic leg differences or guarantee stable drift, and increase the time between RCR legs. Matched shared measurement367.882s gives a simple3× estimate1103.646s, below2700s; this is not an upper-bound guarantee. All-groups21 is unsupported: stable1276.610s×3 exceeds2700s, with its fixed setup share unmeasured.

This is a candidate, not an adopted correction. It requires independent BEFORE review, new protocol/source/validator/tests, full1638-row coverage, fresh five-session reservations and original v5 non-adoption. Any subsequent invalid, indeterminate or numeric failure must stop. No threshold widening or unchanged retry follows from this diagnosis.
