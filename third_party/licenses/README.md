# Verified license overrides

The release inventory normally reads license and notice files directly from each locked installed package. An override is allowed only when a published package declares a license but omits the corresponding text.

`react-remove-scroll-bar@2.3.8` declares MIT in its npm manifest but its published file list excludes `LICENSE`. Its npm integrity is `sha512-9r+yi9+mgU33AKcj6IbT9oRCO78WriSj6t/cF8DWBZJ9aOGPOTEDvdUDz1FwKim7QXWwmHqtdHnRJfhAxEG46Q==`; npm reports git head `b3b1287aad81def2e2ae707274b74531b61ddbaf`. The vendored override was verified on 2026-08-25 against the upstream repository's MIT license at commit [`8ca9ba5ea52de03308fe8ced94f7b159a44d28ff`](https://github.com/theKashey/react-remove-scroll-bar/blob/8ca9ba5ea52de03308fe8ced94f7b159a44d28ff/LICENSE).
