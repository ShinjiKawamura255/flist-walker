# Read-only v0.31.1 evidence observers

These retained sources document the independent artifact and metadata observations; they are not supported product commands. They never execute downloaded binaries. Copy to an owned temporary folder with their original `.py` names to inspect them; signature inspection needs cryptography. Repository validation continues to use the maintained scripts under `scripts/`.

`verify_bundle.py` is unchanged from the independently reviewed v0.31.0 observer. Existing negative controls remain eligible for reuse; each actual new bundle is verified afresh with explicit version0.31.1 and the public predecessor key. The metadata observer only parameterizes version/source/annotated-tag identity while preserving all binding gates. The public downloader only changes its exact version assertion. New collector/outer-archive/log/official-identity observers were independently reviewed for this release.

The initial local official-identity probe assumed unquoted YAML and a runner requirement in the Action README; those assertions stopped the probe. Inspection confirmed `using: 'node24'` and the official runner v2.327.1 release's Node24 support. The final observer checks those actual primary sources; no hosted result, warning or gate was suppressed. Full Action code was freshly fetched and its digest matched the retained causal diagnosis.

Raw logs, API transport files, ZIPs, executables and screenshots remain outside the repository. Retained reports bind sanitized outcomes to exact runs, attempts, source, artifact identities and hashes. Windows native execution is reported separately and is not inferred from these static inspectors.
