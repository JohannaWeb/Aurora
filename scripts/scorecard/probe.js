// Scorecard probe: run by scripts/yt_scorecard.py through AURORA_PROBE_JS
// after the boot pump. Walks the composed tree (shadow roots included) and
// prints one `[scorecard] {json}` line.
(function () {
    // Renderers that hold one result item. On a correct page these sit in a
    // list container (#contents, #items), never directly under a shadow root.
    var ITEM = /^(ytd-(video|rich-item|compact-video|reel-item|playlist|channel|radio|grid-video)-renderer|ytm-shorts-lockup-view-model(-v2)?|yt-lockup-view-model)$/;

    var elements = 0;
    var shadowRoots = 0;
    var textChars = 0;
    var renderers = 0;
    var items = 0;
    var misplacedItems = 0;
    var videoIds = {};

    function videoIdOf(node) {
        try {
            var data = node.data || (node.__data && node.__data.data);
            if (data) {
                return data.videoId ||
                    (data.onTap && data.onTap.innertubeCommand &&
                        data.onTap.innertubeCommand.reelWatchEndpoint &&
                        data.onTap.innertubeCommand.reelWatchEndpoint.videoId) ||
                    null;
            }
        } catch (e) {}
        return null;
    }

    function walk(node, inScript) {
        if (!node) return;
        if (node.nodeType === 3) {
            if (!inScript) textChars += (node.data || '').trim().length;
            return;
        }
        if (node.nodeType === 1) {
            elements++;
            var tag = node.localName || '';
            inScript = inScript || tag === 'script' || tag === 'style';
            if (/-renderer$|-view-model(-v2)?$/.test(tag)) renderers++;
            if (ITEM.test(tag)) {
                items++;
                var parent = node.parentNode;
                if (parent && parent.nodeType === 11) misplacedItems++;
                var id = videoIdOf(node);
                if (id) videoIds[id] = (videoIds[id] || 0) + 1;
            }
        }
        var shadow = null;
        try { shadow = node.shadowRoot || node.__shady_shadowRoot || null; } catch (e) {}
        if (shadow && shadow !== node) {
            shadowRoots++;
            walk(shadow, inScript);
        }
        var children = node.childNodes;
        for (var i = 0; children && i < children.length; i++) walk(children[i], inScript);
    }
    walk(document.documentElement, false);

    var duplicated = 0;
    for (var key in videoIds) if (videoIds[key] > 1) duplicated++;

    function height(el) {
        try {
            var r = el && el.getBoundingClientRect();
            return r ? Math.round(r.height) : 0;
        } catch (e) { return 0; }
    }
    var app = document.querySelector('ytd-app');

    console.log('[scorecard] ' + JSON.stringify({
        elements: elements,
        shadow_roots: shadowRoots,
        text_chars: textChars,
        renderers: renderers,
        items: items,
        distinct_videos: Object.keys(videoIds).length,
        duplicated_videos: duplicated,
        misplaced_items: misplacedItems,
        height: Math.max(height(document.documentElement), height(document.body), height(app)),
        title: document.title || ''
    }));
})();
