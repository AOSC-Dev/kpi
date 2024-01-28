kpi
===

> 你看看你，怎么又在刷 KPI？

A simple tool to list AOSC-Dev contributors during a specified interval.

Usage
-----

Set your GitHub Application Token via the `$GITHUB_TOKEN` environmental variable, and simply:

```
cargo run --release -- --org aosc-dev --days 31 --filter-org-user
```

and run with `--help` for more information.
