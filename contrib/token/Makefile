.POSIX:

DLTOOL = wget -nv --show-progress -O-

LISTS = \
	darkfi_token_list \
	bitcoin_token_list \
	erc20_token_list \
	solana_token_list \
	monero_token_list

LISTS_JSON = $(LISTS:=.json)
LISTS_MIN  = $(LISTS:=.min.json)

ERC20 = https://tokens.coingecko.com/uniswap/all.json
SOL   = https://github.com/solana-labs/token-list/raw/main/src/tokens/solana.tokenlist.json

all: $(LISTS_MIN)

.json.min.json:
	jq -r -c '.' < $< > $@

darkfi_token_list.json:
	git checkout -- $@

bitcoin_token_list.json:
	git checkout -- $@

monero_token_list.json:
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

.SUFFIXES: .json .min.json
.PHONY: all
