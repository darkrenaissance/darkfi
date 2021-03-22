(def! inc (fn* [a] (i+ a 1)))
(def! gensym
  (let* [counter (atom 0)]
    (fn* []
      (symbol (str "G__" (swap! counter inc))))))

(def! gensym2
  (let* [counter (atom 0)]
    (fn* [name]
      (symbol (str name "__" (swap! counter inc))))))
;; Like load-file, but will never load the same path twice.

;; This file is normally loaded with `load-file`, so it needs a
;; different mechanism to neutralize multiple inclusions of
;; itself. Moreover, the file list should never be reset.

(def! load-file-once
  (try*
    load-file-once
  (catch* _
    (let* [seen (atom {"../lib/util.mal" nil})]
      (fn* [filename]
        (if (not (contains? @seen filename))
          (do
            (swap! seen assoc filename nil)
            (load-file filename))))))))
