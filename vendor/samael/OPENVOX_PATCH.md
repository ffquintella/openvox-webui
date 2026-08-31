# OpenVox WebUI patch

This directory contains the crates.io source for `samael` 0.0.22.

OpenVox WebUI builds Samael without its `xmlsec` feature. Version 0.0.22
accidentally gates the `quick_xml` imports used by its non-`xmlsec` response
validation code behind that feature. The local patch removes that one feature
gate so the supported no-`xmlsec` build continues to compile.

The vendored source also includes compiler-suggested lifetime annotations and
an unused-import cleanup so it remains warning-free under the workspace's
`-D warnings` Clippy policy. No runtime behavior is changed by those cleanups.

Remove this override when Samael publishes the fix upstream.
