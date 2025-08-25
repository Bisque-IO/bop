package movd

import "core:testing"

@(test)
test_state_mgr_make :: proc(t: ^testing.T) {
	sm, err := state_mgr_make("state.db")
	ensure(err == nil)
	state_mgr_destroy(sm)

	sm, err = state_mgr_make("")
	ensure(err == nil)
	state_mgr_destroy(sm)
}
