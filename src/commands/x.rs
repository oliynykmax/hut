// ── cmd_x ──────────────────────────────────────────────────────────



use hut::error::HutResult;

pub fn cmd_x(pkg: &str, args: &[String]) -> HutResult<()> {
    hut::fetcher::fetch_and_run(pkg, args)
}
