(function(){
var q=document.getElementById('q'),hits=document.getElementById('hits'),navs=document.getElementById('navs'),idx=null,sel=-1;
var btn=document.querySelector('.menu'),side=document.getElementById('side');
if(btn)btn.onclick=function(){var o=side.classList.toggle('open');btn.setAttribute('aria-expanded',o)};
function load(cb){if(idx)return cb();fetch('search.json').then(function(r){return r.json()}).then(function(j){idx=j;cb()}).catch(function(){idx=[];cb()})}
function run(){var t=q.value.trim().toLowerCase();if(!t){hits.hidden=true;navs.hidden=false;return}
load(function(){var words=t.split(/\s+/),r=[];idx.forEach(function(p){var s=0;words.forEach(function(w){if(p.t.toLowerCase().indexOf(w)>=0)s+=10;if(p.h.toLowerCase().indexOf(w)>=0)s+=4;var c=p.x.split(w).length-1;s+=Math.min(c,12)});
 var ok=words.every(function(w){return (p.t+' '+p.h+' '+p.x).toLowerCase().indexOf(w)>=0});if(ok)r.push([s,p])});
 r.sort(function(a,b){return b[0]-a[0]});hits.innerHTML='';sel=-1;
 r.slice(0,8).forEach(function(x){var p=x[1],li=document.createElement('li'),a=document.createElement('a');a.href=p.u;a.textContent=p.t;var s=document.createElement('small');s.textContent=p.g;a.appendChild(s);li.appendChild(a);hits.appendChild(li)});
 if(!r.length){var li=document.createElement('li');li.style.padding='8px 10px';li.textContent='Nothing found';hits.appendChild(li)}
 hits.hidden=false;navs.hidden=true})}
if(q){q.addEventListener('input',run);q.addEventListener('keydown',function(ev){var a=hits.querySelectorAll('a');if(ev.key==='ArrowDown'||ev.key==='ArrowUp'){ev.preventDefault();if(!a.length)return;sel=(sel+(ev.key==='ArrowDown'?1:-1)+a.length)%a.length;a.forEach(function(x,i){x.classList.toggle('sel',i===sel)})}
 else if(ev.key==='Enter'&&a.length){location.href=a[Math.max(sel,0)].href}else if(ev.key==='Escape'){q.value='';run()}});
 addEventListener('keydown',function(ev){if(ev.key==='/'&&document.activeElement!==q&&!/input|textarea/i.test(document.activeElement.tagName)){ev.preventDefault();q.focus()}})}
var links=[].slice.call(document.querySelectorAll('.toc a'));
if(links.length&&'IntersectionObserver' in window){var map={};links.forEach(function(a){map[a.getAttribute('href').slice(1)]=a});
 var io=new IntersectionObserver(function(es){es.forEach(function(e){if(e.isIntersecting){links.forEach(function(a){a.classList.remove('on')});var a=map[e.target.id];if(a)a.classList.add('on')}})},{rootMargin:'-80px 0px -70% 0px'});
 document.querySelectorAll('.wk h2[id],.wk h3[id]').forEach(function(h){io.observe(h)})}
})();
