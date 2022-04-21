.POSIX:

DLTOOL = wget -nv --show-progress -O-

LISTS = \
	bitcoin_token_list \
	erc20_token_list \
	solana_token_list

LISTS_JSON = $(LISTS:=.json)
LISTS_MIN  = $(LISTS:=.min.json)

ERC20 = https://tokens.coingecko.com/uniswap/all.json
SOL   = https://github.com/solana-labs/token-list/raw/main/src/tokens/solana.tokenlist.json

all: $(LISTS_MIN)

$(LISTS_MIN): $(LISTS_JSON)
	jq -r -c < $< > $@

bitcoin_token_list.json:
	git checkout -- $@

erc20_token_list.json:
	$(DLTOOL) $(ERC20) | jq -r > $@

solana_token_list.json:
	$(DLTOOL) $(SOL) | sed \
		-e 's@"symbol": "BTC"@"symbol": "SBTC"@' \
		-e 's@"symbol": "ETH"@"symbol": "SETH"@' \
		| jq -r > $@

clean:
	rm -f $(LISTS_JSON) $(LISTS_MIN)

.PHONY: all
