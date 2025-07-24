package concurrent

import "core:fmt"
import "core:log"
import "core:testing"
import "core:time"

@(test)
test_map :: proc(t: ^testing.T) {
	time.sleep(time.Millisecond)
	cm, err := map_create(u64, u64, 8)
	testing.expect(t, err == nil)
	defer map_destroy(cm)
	map_put(cm, 1, 2)
	v, ok := map_get(cm, 1)
	log.info(v, ok)
	v, ok = map_remove(cm, 1)
	log.info(v, ok)
	v, ok = map_get(cm, 1)
	log.info(v, ok)
	map_put(cm, 1, 2)
}
