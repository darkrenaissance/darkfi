.POSIX:

DLTOOL = wget -nv --show-progress -O-

LISTS = \
	erc20_token_list.json \
	solana_token_list.json

ERC20 = https://tokens.coingecko.com/uniswap/all.json
SOL = https://github.com/solana-labs/token-list/raw/main/src/tokens/solana.tokenlist.json

all: $(LISTS)

erc20_token_list.json:
	$(DLTOOL) $(ERC20) | jq -r > $@

solana_token_list.json:
	$(DLTOOL) $(SOL) | sed \
		-e 's@"symbol": "BTC"@"symbol": "SBTC"@' \
		-e 's@"symbol": "ETH"@"symbol": "SETH"@' \
		| jq -r > $@

.PHONY: all $(LISTS)
