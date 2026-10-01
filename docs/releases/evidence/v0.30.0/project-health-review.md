# v0.30.0 Project Health Review

## Identity and method
Reviewed clean source `2138076dfea1a63c986d4f8195717a08fb569300`, base `48798929a73cc49411c5d7c50265efd743c33e3c`, after the preceding fix task explicitly completed. Work is in an independent checkout. Latest public release at review was v0.29.0. Main reviewed forward ownership/execution flows; an independent read-only reviewer traced contracts, failure paths and test evidence without implementation involvement. The source was fixed during both reads. This is a diagnostic record, not release approval.

## Findings and disposition
| ID | Severity / confidence | Evidence | Necessity / disposition |
|---|---|---|---|
| H-01 | Medium / high | `scripts/check-updater-n-minus-one-compatibility.py` omitted shipped 0.29.0; added exact 26-entry 0.29.0→0.30.0 test failed with unsupported previous capability | Fix required: candidate assembly fails closed. Registered only the two families proven by the immutable v0.29.0 parser. New regression and existing checker cases pass; generated signed candidate manifest remains a separate pending gate. |
| H-02 | Release metadata / high | Cargo/CHANGELOG still 0.29.0 and Unreleased had no entries for two features and subsequent fixes | Prepare 0.30.0 under existing minor-version policy; classify the complete v0.29.0..target range. No breaking behavior proposed. |

No additional Critical/High product defect was established. Absence of a diagnosed defect does not establish runtime, native or publication PASS.

## Coverage and evidence
| Lens | Inspected boundaries / contracts and tests | Assessment / limitation |
|---|---|---|
| Purpose and structure | AGENTS, canonical docs, app/worker/persistence owners, architecture boundary tests | Existing module/contract ownership matches purpose; no justified broad restructure. |
| Correctness and regression | query compiled/evaluator/rank, result parity, active_filter identity and subset proof, incremental empty-query/cancel tests | Shared contracts and exception boundaries are explicit. Final whole-suite verification is recorded separately. |
| Concurrency, cancel and shutdown | worker runtime/bus, stale routing, index mailbox, full/disconnected retirement rollback, single physical freshness slot | Logical timeout retains physical occupancy, avoiding overlapping unbounded OS probes. Blocked OS I/O remains a documented degraded mode. Panic differs from healthy settlement. |
| Persistence and recovery | bounded persistence admission, document JSON protection/generation, settings paired rollback, fs_atomic tests | Malformed/unknown fields preserved; failure remains observable. Settings files do not acquire a cross-file crash journal; rollback tests prove their scoped contract, not arbitrary crash atomicity. |
| GUI and keyboard | shortcut/modal/IME routing, paged-preview controls, rendered checkbox/TextEdit events, query/ignore updates | Deterministic coverage inspected. Real native input, visual rendering, IME and platform sessions remain separate evidence. |
| Performance and resources | 4 ms/32768 known-kind filter slice; 512 unknown-kind/4096 backlog; bounded workers/mailboxes/caches/preview; ignored latency guard | Limits exist at owner boundaries. A single active dataset has no claimed absolute byte cap. 500k preparation and paused ingestion remain observational risk; final measurements recorded separately. |
| Dependencies and security | Cargo/lock/notices; action whole-request and immediate reauthorization with argv; updater signature-first download bounds/deadlines/redirect/transaction recovery | Lexical linked-root authorization is explicitly permitted by FR-009; no invented stricter contract. Latest audit and four-target resolve evidence are separate execution results. |
| Specs and tests | FR/SP/DES/TC links, VM routing, architecture tests and failure/cancel/generation matrix | No confirmed mismatch beyond H-01. Native gaps remain marked NOT RUN; coverage threshold remains 75%. |
| Build, distribution and update | release workflow candidate/tag/draft gates, exact 28 assets/26 checksums, sidecars and archive contracts, updater N-1 | H-01 is blocking preparation until fixed. Actual candidate/tag/signature/download checks pending; prior release exceptions do not apply. |

## Imported completed fixes
Commit `2138076` was completed with preceding task validation and independent review before import. It fixes candidate preparation latency, ignore membership reevaluation via checkbox/assist/undo, and off-thread scratch retirement preserving Full/Disconnected ownership and terminal state. Its prior evidence is provenance only; release validation uses the final code and exact candidate identity.

## Release posture
Not published. Required source/CI/native/candidate/tag/download evidence is owned by the release packet and must remain NOT RUN until actually executed. Windows session, alternate display/DPI, IME, owned external handler, clipboard gate and updater prerequisites must be inventoried before launch. Unavailable required axes need a concrete v0.30.0-specific user decision. No historical native waiver or warning exception is inherited.
