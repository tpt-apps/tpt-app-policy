;; Tries to open a network connection. The manifest grants no network access,
;; so the runner refuses this module before it runs. Expect exit code 7.
(module
  (import "tpt" "net_connect" (func $connect (param i32) (result i32)))

  (func (export "run") (result i32)
    (call $connect (i32.const 0))))
