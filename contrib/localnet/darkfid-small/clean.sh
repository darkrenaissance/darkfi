#!/bin/sh
rm -rf darkfid0 darkfid1 darkfid2 drk0 drk1 drk2 darkfid0.log darkfid1.log darkfid2.log
sed -i -e "s|XMRIG_USER0=.*|XMRIG_USER0=\"DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf\"|g" tmux_sessions.sh
sed -i -e "s|XMRIG_USER1=.*|XMRIG_USER1=\"Dae4FtyzrnQ8JNuui5ibZL4jXUR786PbyjwBsq4aj6E1RPPYjtXLfnAf\"|g" tmux_sessions.sh
sed -i -e "s|XMRIG_USER0=.*|XMRIG_USER0=\"DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf\"|g" reorg-test.sh
sed -i -e "s|XMRIG_USER1=.*|XMRIG_USER1=\"Dae4FtyzrnQ8JNuui5ibZL4jXUR786PbyjwBsq4aj6E1RPPYjtXLfnAf\"|g" reorg-test.sh
sed -i -e "s|skip_sync =.*|skip_sync = false|g" darkfid1.toml
