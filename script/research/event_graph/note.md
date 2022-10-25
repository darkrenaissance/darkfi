1. Sync is the biggest problem for IRC, **not** ordering.
2. As long as the clock is *somewhat* accurate, then we
   can prune really old data.
3. Eventually messages will have a cost, so attacking the
   event graph will be ineffective (trying to make very old
   branches that nodes will reject).
4. Orphan pool should drop older orphans eventually.
5. We will need good monitoring tools to track the graph.

