(function () {
  'use strict';

  var R = 3000;
  var HIST = 72;
  var cpuHist = new Array(HIST).fill(0);

  var C_CYAN = '#78d6df';
  var C_BLUE = '#6fa4d8';
  var C_GREEN = '#3f9f69';
  var C_YELLOW = '#d7bd55';
  var C_ORANGE = '#d8873e';
  var C_RED = '#d94f55';

  function clamp(v, min, max) {
    return Math.min(Math.max(Number(v) || 0, min), max);
  }
  function clamp01(v) {
    return clamp(v, 0, 1);
  }

  function hexToRgb(hex) {
    var h = String(hex || '').replace('#', '');
    if (h.length === 3)
      h = h
        .split('')
        .map(function (c) {
          return c + c;
        })
        .join('');
    return {
      r: parseInt(h.slice(0, 2), 16) || 0,
      g: parseInt(h.slice(2, 4), 16) || 0,
      b: parseInt(h.slice(4, 6), 16) || 0,
    };
  }

  function rgbToHex(r, g, b) {
    return (
      '#' +
      [r, g, b]
        .map(function (v) {
          var s = Math.round(clamp(v, 0, 255)).toString(16);
          return s.length === 1 ? '0' + s : s;
        })
        .join('')
    );
  }

  function mix(a, b, t) {
    var x = clamp01(t),
      ca = hexToRgb(a),
      cb = hexToRgb(b);
    return rgbToHex(ca.r + (cb.r - ca.r) * x, ca.g + (cb.g - ca.g) * x, ca.b + (cb.b - ca.b) * x);
  }

  function heat(value, min, max) {
    var x = clamp01(((Number(value) || 0) - min) / Math.max(max - min, 1));
    if (x < 0.55) return mix(C_GREEN, C_YELLOW, x / 0.55);
    if (x < 0.78) return mix(C_YELLOW, C_ORANGE, (x - 0.55) / 0.23);
    return mix(C_ORANGE, C_RED, (x - 0.78) / 0.22);
  }

  function alpha(hex, a) {
    var c = hexToRgb(hex);
    return 'rgba(' + c.r + ',' + c.g + ',' + c.b + ',' + a + ')';
  }

  function esc(s) {
    return String(s || '')
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  function fmt(b) {
    b = Number(b) || 0;
    if (b >= 1e12) return (b / 1e12).toFixed(2) + ' TB';
    if (b >= 1e9) return (b / 1e9).toFixed(1) + ' GB';
    if (b >= 1e6) return (b / 1e6).toFixed(0) + ' MB';
    if (b >= 1e3) return (b / 1e3).toFixed(0) + ' KB';
    return b + ' B';
  }

  function up(s) {
    s = Number(s) || 0;
    var d = Math.floor(s / 86400);
    var h = Math.floor((s % 86400) / 3600);
    var m = Math.floor((s % 3600) / 60);
    return d ? d + 'd ' + h + 'h' : h + 'h ' + m + 'm';
  }

  function setText(id, val) {
    var el = document.getElementById(id);
    if (el) el.textContent = val == null || val === '' ? '-' : val;
  }

  function pickTemp(list, keys, fallback) {
    var arr = list || [],
      wanted = keys || [];
    for (var i = 0; i < arr.length; i++) {
      var type = String(arr[i].type || '').toLowerCase();
      for (var j = 0; j < wanted.length; j++) {
        if (type.indexOf(wanted[j]) !== -1) return parseInt(arr[i].temp_mc, 10) / 1000;
      }
    }
    return typeof fallback === 'number' ? fallback : null;
  }

  function firstTemp(list) {
    return list && list.length ? parseInt(list[0].temp_mc, 10) / 1000 : null;
  }

  function cpuTemp(d) {
    return pickTemp(
      d.temperatures,
      ['package', 'cpu', 'coretemp', 'tdie', 'tctl', 'k10temp'],
      firstTemp(d.temperatures),
    );
  }

  function gpuTemp(d) {
    if (d.nvidia && typeof d.nvidia.temp === 'number') return d.nvidia.temp;
    return pickTemp(d.temperatures, ['gpu', 'video'], null);
  }

  function tempWord(v) {
    if (v == null) return 'no sensor';
    if (v >= 80) return 'critical';
    if (v >= 65) return 'hot';
    if (v >= 50) return 'warm';
    return 'stable';
  }

  function buildSegments(id, count) {
    var el = document.getElementById(id);
    if (!el) return;
    var html = '';
    for (var i = 0; i < count; i++) html += '<span class="seg"></span>';
    el.innerHTML = html;
  }

  function setSegments(id, pct) {
    var el = document.getElementById(id);
    if (!el) return;
    var segs = el.querySelectorAll('.seg');
    var fill = Math.round((clamp(pct, 0, 100) / 100) * segs.length);
    var c = heat(pct, 0, 100);
    for (var i = 0; i < segs.length; i++) {
      segs[i].classList.toggle('on', i < fill);
      segs[i].style.setProperty('--seg-color', i < fill ? c : 'rgba(214,218,228,.075)');
    }
  }

  function drawChart(id, points, color) {
    var cv = document.getElementById(id);
    if (!cv) return;
    var wrap = cv.parentElement;
    var W = wrap ? wrap.clientWidth : 400;
    var H = wrap ? wrap.clientHeight : 120;
    var ratio = window.devicePixelRatio || 1;

    cv.width = Math.max(10, W * ratio);
    cv.height = Math.max(10, H * ratio);
    cv.style.width = W + 'px';
    cv.style.height = H + 'px';

    var ctx = cv.getContext('2d');
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.clearRect(0, 0, W, H);

    if (!points || points.length < 2) return;

    ctx.beginPath();
    ctx.moveTo(0, H);
    for (var i = 0; i < points.length; i++) {
      ctx.lineTo((i / (points.length - 1)) * W, H - (points[i] / 100) * H * 0.88 - H * 0.06);
    }
    ctx.lineTo(W, H);
    ctx.closePath();
    ctx.fillStyle = alpha(color, 0.14);
    ctx.fill();

    ctx.beginPath();
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.8;
    for (var j = 0; j < points.length; j++) {
      var x = (j / (points.length - 1)) * W;
      var y = H - (points[j] / 100) * H * 0.88 - H * 0.06;
      if (j === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();

    var last = points[points.length - 1];
    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.arc(W - 2, H - (last / 100) * H * 0.88 - H * 0.06, 3, 0, Math.PI * 2);
    ctx.fill();
  }

  function switchProcTab(tab) {
    var top = document.getElementById('proc-top');
    var all = document.getElementById('proc-all');
    var bt = document.getElementById('tab-top');
    var ba = document.getElementById('tab-all');

    if (top) top.style.display = tab === 'top' ? 'block' : 'none';
    if (all) all.style.display = tab === 'all' ? 'block' : 'none';
    if (bt) bt.classList.toggle('active', tab === 'top');
    if (ba) ba.classList.toggle('active', tab === 'all');
  }
  window.switchProcTab = switchProcTab;

  function update(d) {
    if (!d || !d.cpu || !d.memory) return;

    setText('ts', new Date().toLocaleTimeString('en-GB'));
    setText('hostname', d.hostname || '-');

    var cores = parseInt(d.cpu.cores, 10) || 1;
    var l1 = parseFloat(d.cpu.load_1) || 0;
    var l5 = parseFloat(d.cpu.load_5) || 0;
    var l15 = parseFloat(d.cpu.load_15) || 0;
    var cpuPct = clamp((l1 / cores) * 100, 0, 100);

    var memTot = d.memory.total_bytes || 1;
    var memUsd = d.memory.used_bytes || 0;
    var memPct = clamp((memUsd / memTot) * 100, 0, 100);
    var swapPct = d.memory.swap_total_bytes
      ? clamp((d.memory.swap_used_bytes / d.memory.swap_total_bytes) * 100, 0, 100)
      : 0;

    var ct = cpuTemp(d);
    var gt = gpuTemp(d);

    var cpuColor = heat(cpuPct, 0, 100);
    var memColor = heat(memPct, 0, 100);

    cpuHist.push(cpuPct);
    if (cpuHist.length > HIST) cpuHist.shift();

    var alertItems = [];
    if (cpuPct > 90)
      alertItems.push({ sev: 'crit', txt: 'CPU overload', sub: cpuPct.toFixed(0) + '%' });
    if (memPct > 90)
      alertItems.push({ sev: 'crit', txt: 'Memory pressure', sub: memPct.toFixed(0) + '%' });
    if (swapPct > 80)
      alertItems.push({ sev: 'warn', txt: 'Swap filling', sub: swapPct.toFixed(0) + '%' });

    (d.temperatures || []).forEach(function (t) {
      var temp = parseInt(t.temp_mc, 10) / 1000;
      if (temp > 80)
        alertItems.push({
          sev: 'crit',
          txt: 'High temperature',
          sub: (t.type || 'sensor') + ' ' + temp.toFixed(0) + 'C',
        });
    });

    var pressure = Math.max(cpuPct, memPct, swapPct, ct || 0, gt || 0);
    var state = 'Stable';
    var stateSub = 'System is within normal operating range.';
    var pressureFill = 28;
    var pressureColor = C_GREEN;

    if (alertItems.length) {
      state = 'Warning';
      stateSub = 'Check active warnings and thermal/load pressure.';
      pressureFill = 58;
      pressureColor = C_YELLOW;
    }
    if (cpuPct > 90 || memPct > 90 || (ct != null && ct > 82) || (gt != null && gt > 82)) {
      state = 'Hot';
      stateSub = 'Load or heat is above the comfort zone.';
      pressureFill = 92;
      pressureColor = C_RED;
    }

    setText('state-value', state);
    setText('state-sub', stateSub);
    setText('status-tag', d.os || 'local node');
    setText('uptime-val', up(d.uptime_sec || 0));
    setText('status-load', cpuPct.toFixed(0) + '%');
    setText('status-memory', memPct.toFixed(0) + '%');
    setText('side-cpu-temp', ct == null ? '-' : ct.toFixed(0) + 'C');
    setText('side-gpu-temp', gt == null ? '-' : gt.toFixed(0) + 'C');
    setText('side-proc', d.processes_count || 0);

    var sbf = document.getElementById('state-bar-fill');
    if (sbf) {
      sbf.style.width = pressureFill + '%';
      sbf.style.setProperty('--fill', pressureColor);
      sbf.style.background = pressureColor;
    }

    var heatTag = [];
    if (ct != null) heatTag.push('CPU ' + tempWord(ct));
    if (gt != null) heatTag.push('GPU ' + tempWord(gt));
    setText('heat-tag', heatTag.join(' / ') || 'no sensors');

    var heatEl = document.getElementById('heat-list');
    if (heatEl) {
      var sensors = (d.temperatures || []).slice(0, 10);
      heatEl.innerHTML = sensors.length
        ? sensors
            .map(function (t) {
              var temp = parseInt(t.temp_mc, 10) / 1000;
              var pct = clamp(((temp - 30) / 65) * 100, 0, 100);
              var c = heat(temp, 35, 90);
              return (
                '<div class="heat-row">' +
                '<div class="heat-name" title="' +
                esc(t.type || '') +
                '">' +
                esc(t.type || 'sensor') +
                '</div>' +
                '<div class="heat-meter"><div class="heat-fill ' +
                (temp >= 72 ? 'hot' : '') +
                '" style="width:' +
                pct +
                '%;--fill:' +
                c +
                ';background:' +
                c +
                '"></div></div>' +
                '<div class="heat-temp" style="color:' +
                c +
                '">' +
                temp.toFixed(0) +
                'C</div>' +
                '</div>'
              );
            })
            .join('')
        : '<div class="ok-line">No temperature sensors</div>';
    }

    setText('alert-count', alertItems.length ? alertItems.length + ' active' : 'clear');
    var alertEl = document.getElementById('alert-list');
    if (alertEl) {
      if (alertItems.length) {
        alertEl.innerHTML = alertItems
          .map(function (a) {
            var c = a.sev === 'crit' ? C_RED : C_YELLOW;
            return (
              '<div class="alert-item" style="--accent:' +
              c +
              '"><strong>' +
              esc(a.txt) +
              '</strong><span>' +
              esc(a.sub) +
              '</span></div>'
            );
          })
          .join('');
      } else {
        alertEl.innerHTML = '<div class="ok-line">All clear</div>';
      }
    }

    setText('cpu-title', 'Load');
    setText('cpu-info', cores + ' cores · ' + ((d.cpu.freq_avg_hz || 0) / 1e6).toFixed(0) + ' MHz');

    var cpuVal = document.getElementById('cpu-load-val');
    if (cpuVal) {
      cpuVal.textContent = cpuPct.toFixed(1) + '%';
      cpuVal.style.color = cpuColor;
    }

    setSegments('cpu-meter', cpuPct);
    drawChart('cpu-chart', cpuHist, cpuColor);

    var coreEl = document.getElementById('core-lanes');
    if (coreEl) {
      var freqs = d.cpu.per_core_freqs || [];
      var maxF = 0;
      freqs.forEach(function (f) {
        maxF = Math.max(maxF, Number(f.cur_mhz) || 0);
      });
      maxF = maxF || 3000;

      coreEl.innerHTML = freqs
        .slice(0, 8)
        .map(function (f) {
          var pct = clamp(((Number(f.cur_mhz) || 0) / maxF) * 100, 0, 100);
          var c = heat(pct, 0, 100);
          return (
            '<div class="core-lane">' +
            '<label>C' +
            esc(f.core) +
            '</label>' +
            '<strong>' +
            esc(f.cur_mhz) +
            ' MHz</strong>' +
            '<div class="core-fill"><span style="width:' +
            pct +
            '%;--fill:' +
            c +
            ';background:' +
            c +
            '"></span></div>' +
            '</div>'
          );
        })
        .join('');
    }

    setText('mem-info', fmt(memUsd) + ' / ' + fmt(memTot));
    var used = document.getElementById('slab-used');
    var cache = document.getElementById('slab-cache');
    var free = document.getElementById('slab-free');

    var cachePct = clamp(((d.memory.cached_bytes || 0) / memTot) * 100, 0, 100);
    var freePct = Math.max(0, 100 - memPct - cachePct);

    if (used) {
      used.style.width = memPct + '%';
      used.style.setProperty('--part', memColor);
      used.style.background = memColor;
    }
    if (cache) cache.style.width = cachePct + '%';
    if (free) free.style.width = freePct + '%';

    var memList = document.getElementById('mem-list');
    if (memList) {
      var items = [
        ['Used', fmt(memUsd)],
        ['Avail', fmt(d.memory.avail_bytes || 0)],
        ['Cached', fmt(d.memory.cached_bytes || 0)],
        ['Swap', swapPct.toFixed(0) + '%'],
      ];
      memList.innerHTML = items
        .map(function (it) {
          return (
            '<div class="mini"><label>' + it[0] + '</label><strong>' + it[1] + '</strong></div>'
          );
        })
        .join('');
    }

    setText('load-1', l1.toFixed(2));
    setText('load-5', l5.toFixed(2));
    setText('load-15', l15.toFixed(2));

    ['load-1', 'load-5', 'load-15'].forEach(function (id, idx) {
      var el = document.getElementById(id);
      if (el) {
        var v = [cpuPct, clamp((l5 / cores) * 100, 0, 100), clamp((l15 / cores) * 100, 0, 100)][
          idx
        ];
        el.style.color = heat(v, 0, 100);
      }
    });

    var procs = d.processes || [];
    setText('proc-count', (d.processes_count || 0) + ' total');

    var topBody = document.getElementById('proc-top-body');
    var allBody = document.getElementById('proc-all-body');

    if (topBody) {
      topBody.innerHTML = procs
        .slice(0, 8)
        .map(function (p) {
          var cc = heat(p.cpu_pct || 0, 0, 100);
          var rc = heat(p.mem_pct || 0, 0, 100);
          return (
            '<tr>' +
            '<td title="' +
            esc(p.name || '') +
            '">' +
            esc(p.name || '') +
            '</td>' +
            '<td style="text-align:right;color:' +
            cc +
            ';font-weight:850">' +
            (p.cpu_pct || 0).toFixed(1) +
            '%</td>' +
            '<td style="text-align:right;color:' +
            rc +
            ';font-weight:850">' +
            (p.ram_mb || 0) +
            '</td>' +
            '</tr>'
          );
        })
        .join('');
    }

    if (allBody) {
      allBody.innerHTML = procs
        .slice(0, 50)
        .map(function (p) {
          var cc = heat(p.cpu_pct || 0, 0, 100);
          var rc = heat(p.mem_pct || 0, 0, 100);
          return (
            '<tr>' +
            '<td title="' +
            esc(p.name || '') +
            '">' +
            esc(p.name || '') +
            '</td>' +
            '<td style="text-align:right;color:' +
            cc +
            ';font-weight:850">' +
            (p.cpu_pct || 0).toFixed(1) +
            '%</td>' +
            '<td style="text-align:right;color:' +
            rc +
            ';font-weight:850">' +
            (p.ram_mb || 0) +
            '</td>' +
            '<td style="text-align:right;color:' +
            rc +
            '">' +
            (p.mem_pct || 0).toFixed(1) +
            '%</td>' +
            '</tr>'
          );
        })
        .join('');
    }

    var netEl = document.getElementById('net-list');
    if (netEl) {
      var rows = '';
      (d.network || []).forEach(function (n) {
        if (n.name === 'lo') return;
        var rx = (n.rx_rate || 0) / 1e6;
        var tx = (n.tx_rate || 0) / 1e6;
        var rxPct = clamp(rx * 20, 0, 100);
        var txPct = clamp(tx * 20, 0, 100);

        rows +=
          '<div class="net-row">' +
          '<div class="net-top"><div><div class="net-name">' +
          esc(n.name || '') +
          '</div><div class="net-ip">' +
          esc(n.ip4 || '') +
          '</div></div><div class="net-ip">↓ ' +
          rx.toFixed(2) +
          ' / ↑ ' +
          tx.toFixed(2) +
          '</div></div>' +
          '<div class="signal ' +
          (rx > 0.01 ? 'active' : '') +
          '"><span style="width:' +
          rxPct +
          '%;--fill:' +
          C_BLUE +
          ';background:' +
          C_BLUE +
          '"></span></div>' +
          '<div class="signal ' +
          (tx > 0.01 ? 'active' : '') +
          '"><span style="width:' +
          txPct +
          '%;--fill:' +
          C_ORANGE +
          ';background:' +
          C_ORANGE +
          '"></span></div>' +
          '</div>';
      });
      netEl.innerHTML = rows || '<div class="ok-line">No network interfaces</div>';
    }

    var diskEl = document.getElementById('disk-list');
    if (diskEl) {
      var disks = '';
      var count = 0;
      (d.mounts || []).forEach(function (m) {
        if (!(m.fs || '').startsWith('/dev')) return;
        count++;
        var pct = clamp(m.pct || 0, 0, 100);
        var c = heat(pct, 0, 100);

        disks +=
          '<div class="disk-row">' +
          '<div class="disk-top"><div><div class="disk-name" title="' +
          esc(m.mount || '') +
          '">' +
          esc(m.mount || '') +
          '</div><div class="disk-sub">' +
          fmt(m.used || 0) +
          ' used · ' +
          fmt(m.total || 0) +
          ' total</div></div><div class="disk-sub" style="color:' +
          c +
          ';font-weight:900">' +
          pct +
          '%</div></div>' +
          '<div class="disk-fill"><span style="width:' +
          pct +
          '%;--fill:' +
          c +
          ';background:' +
          c +
          '"></span></div>' +
          '</div>';
      });
      setText('disk-tag', count ? count + ' mounts' : '-');
      diskEl.innerHTML = disks || '<div class="ok-line">No disks</div>';
    }

    var gpuEl = document.getElementById('gpu-list');
    if (gpuEl) {
      var nv = d.nvidia || {};
      var hasNv = nv && Object.keys(nv).length > 0;
      var gpus = d.gpu || [];
      setText('gpu-tag', gpus.length ? gpus.length + ' device(s)' : '-');

      gpuEl.innerHTML = gpus.length
        ? gpus
            .map(function (g) {
              var name = (g.name || '').split('controller: ')[1] || g.name || 'GPU';
              var util = hasNv && name.indexOf('NVIDIA') !== -1 ? nv.util || 0 : 0;
              var c = heat(util, 0, 100);
              var details =
                hasNv && name.indexOf('NVIDIA') !== -1
                  ? 'VRAM ' +
                    (nv.mem_used || 0) +
                    '/' +
                    (nv.mem_total || 0) +
                    ' · ' +
                    (nv.temp || 0) +
                    'C · ' +
                    (nv.power_w || 0) +
                    'W'
                  : esc(g.pci || '');

              return (
                '<div class="gpu-row">' +
                '<div class="gpu-name" title="' +
                esc(name) +
                '">' +
                esc(name) +
                '</div>' +
                '<div class="gpu-sub">' +
                details +
                '</div>' +
                '<div class="signal"><span style="width:' +
                util +
                '%;--fill:' +
                c +
                ';background:' +
                c +
                '"></span></div>' +
                '</div>'
              );
            })
            .join('')
        : '<div class="ok-line">No GPU</div>';
    }

    var powerEl = document.getElementById('power-list');
    if (powerEl) {
      var bat = (d.battery || [])[0] || null;
      if (bat) {
        var bp = clamp(bat.capacity_pct || 0, 0, 100);
        var bc = heat(100 - bp, 0, 100);
        powerEl.innerHTML =
          '<div class="power-row">' +
          '<label>Battery</label>' +
          '<strong style="color:' +
          bc +
          '">' +
          bp +
          '%</strong>' +
          '<div class="sys-sub">' +
          esc(bat.status || '-') +
          '</div>' +
          '<div class="signal"><span style="width:' +
          bp +
          '%;--fill:' +
          bc +
          ';background:' +
          bc +
          '"></span></div>' +
          '</div>';
        setText('power-tag', bat.status || '-');
      } else {
        powerEl.innerHTML = '<div class="ok-line">No battery</div>';
        setText('power-tag', 'wall');
      }
    }

    var sys = document.getElementById('sys-grid');
    if (sys) {
      var sysItems = [
        ['Host', d.hostname],
        ['OS', d.os],
        ['Kernel', d.kernel],
        ['Arch', d.arch],
        ['Cores', d.cpu.cores],
        ['Threads', d.cpu.threads_per_core],
        ['Avg freq', d.cpu.freq_avg_hz ? (d.cpu.freq_avg_hz / 1e6).toFixed(0) + ' MHz' : '-'],
        ['L2', d.cpu.cache_l2],
        ['L3', d.cpu.cache_l3],
      ];
      sys.innerHTML = sysItems
        .map(function (it) {
          return (
            '<div class="sys-row"><label>' +
            esc(it[0]) +
            '</label><strong title="' +
            esc(it[1] || '-') +
            '">' +
            esc(it[1] || '-') +
            '</strong></div>'
          );
        })
        .join('');
      setText('system-tag', d.arch || '-');
    }
  }

  buildSegments('cpu-meter', 26);

  function desktopBridgeShape(stats) {
    if (!stats) return null;
    var cores = Number(stats.cpuCores || 1) || 1;
    var cpuPct = Number(stats.cpuUsagePercent || 0) || 0;
    var memoryTotal = Number(stats.memoryTotalBytes || 0) || 1;
    var memoryUsed = Number(stats.memoryUsedBytes || 0) || 0;
    var networkReceived = Number(stats.networkReceivedBytes || 0) || 0;
    var networkTransmitted = Number(stats.networkTransmittedBytes || 0) || 0;
    return {
      hostname: stats.hostName || '-',
      os: [stats.osName, stats.osVersion].filter(Boolean).join(' ') || 'Desktop bridge',
      kernel: stats.kernelVersion || '-',
      arch: stats.arch || '-',
      uptime_sec: Number(stats.uptimeSeconds || 0) || 0,
      processes_count: Number(stats.processCount || 0) || 0,
      cpu: {
        cores: cores,
        threads_per_core: 1,
        load_1: (cpuPct / 100) * cores,
        load_5: (cpuPct / 100) * cores,
        load_15: (cpuPct / 100) * cores,
        freq_avg_hz: Number(stats.cpuFrequencyHz || 0) || 0,
        per_core_freqs: [],
      },
      memory: {
        total_bytes: memoryTotal,
        used_bytes: memoryUsed,
        avail_bytes: Math.max(memoryTotal - memoryUsed, 0),
        cached_bytes: 0,
        swap_total_bytes: Number(stats.swapTotalBytes || 0) || 0,
        swap_used_bytes: Number(stats.swapUsedBytes || 0) || 0,
      },
      temperatures:
        stats.temperatureCelsius == null
          ? []
          : [{ type: 'cpu', temp_mc: Number(stats.temperatureCelsius) * 1000 }],
      mounts: (stats.disks || []).map(function (disk) {
        var total = Number(disk.totalBytes || 0) || 0;
        var available = Number(disk.availableBytes || 0) || 0;
        return {
          fs: '/dev/desktop',
          mount: disk.mountPoint || disk.name || 'Disk',
          total: total,
          used: Math.max(total - available, 0),
          pct: Number(disk.usedPercent || 0) || 0,
        };
      }),
      network: [
        {
          name: 'desktop',
          ip4: 'local',
          rx_rate: networkReceived,
          tx_rate: networkTransmitted,
        },
      ],
      processes: [],
    };
  }

  window.addEventListener('message', function (event) {
    var payload = event && event.data;
    if (!payload || payload.type !== 'argentum-system-stats') return;
    var shaped = desktopBridgeShape(payload.stats);
    if (shaped) update(shaped);
  });

  async function tick() {
    try {
      var r = await fetch('/data', { cache: 'no-cache' });
      if (!r.ok) return;
      var d = await r.json();
      update(d);
    } catch (err) {
      console.warn('tick error:', err);
    }
  }

  tick();
  setInterval(tick, R);

  window.addEventListener('resize', function () {
    drawChart('cpu-chart', cpuHist, C_GREEN);
  });
})();
