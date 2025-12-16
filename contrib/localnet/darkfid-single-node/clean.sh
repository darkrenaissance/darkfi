#!/bin/sh
rm -rf darkfid drk
sed -i -e "s|recipient =.*|recipient = \"DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf\"|g" minerd.toml
