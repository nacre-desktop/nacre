import QtQuick
import QtTest

TestCase {
    id: test
    name: "IdleContentLifetime"
    when: windowShown
    width: 300
    height: 200
    Component {
        id: content
        Item {
            property string query: ""
            function captureState() {
                return {
                    query: query
                };
            }
            function restoreState(data) {
                query = data.query;
            }
        }
    }
    Component {
        id: holder
        NacreIdleLoader {
            sourceComponent: content
            releaseDelay: 80
            preserveState: true
        }
    }
    function test_no_eager_creation_reuse_cancel_release_and_restore() {
        const loader = createTemporaryObject(holder, test);
        verify(!loader.item);
        loader.wanted = true;
        verify(loader.item);
        const first = loader.item;
        first.query = "unchanged draft";
        loader.wanted = false;
        wait(30);
        compare(loader.item, first);
        loader.wanted = true;
        wait(100);
        compare(loader.item, first);
        loader.wanted = false;
        tryCompare(loader, "active", false, 500);
        verify(!loader.item);
        compare(loader.savedState.query, "unchanged draft");
        loader.wanted = true;
        verify(loader.item);
        compare(loader.item.query, "unchanged draft");
    }
    function test_created_while_already_open_keeps_its_warm_period() {
        const loader = createTemporaryObject(holder, test, {
            wanted: true
        });
        verify(loader.item);
        loader.wanted = false;
        wait(30);
        verify(loader.item);
        tryCompare(loader, "active", false, 500);
    }
    function test_warm_policy_can_be_trimmed_after_switching_content() {
        const loader = createTemporaryObject(holder, test, {
            keepWarm: true
        });
        verify(!loader.item);
        loader.wanted = true;
        const item = loader.item;
        loader.wanted = false;
        wait(120);
        compare(loader.item, item);
        loader.keepWarm = false;
        tryCompare(loader, "active", false, 500);
    }
    function test_reset_content_when_preservation_disabled() {
        const loader = createTemporaryObject(holder, test, {
            preserveState: false
        });
        loader.wanted = true;
        loader.item.query = "previous search";
        loader.wanted = false;
        tryCompare(loader, "active", false, 500);
        loader.wanted = true;
        compare(loader.item.query, "");
    }
}
