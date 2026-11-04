#!/usr/bin/env bash
# rustport survey — inventory a source tree before porting.
# Usage: survey.sh <src_dir>
set -u
d="${1:?usage: survey.sh <src_dir>}"
cd "$d" || exit 1
ex='-not -path */node_modules/* -not -path */.git/* -not -path */target/* -not -path */build/* -not -path */Library/* -not -path */Pods/*'

echo "== Languages (files / LOC) =="
for e in c h cpp cc cxx hpp cs java kt swift m mm js ts tsx py lua gd go rs glsl hlsl wgsl vert frag shader; do
  n=$(find . -type f -name "*.$e" $ex 2>/dev/null | wc -l | tr -d ' ')
  [ "$n" -gt 0 ] || continue
  loc=$(find . -type f -name "*.$e" $ex -print0 2>/dev/null | xargs -0 cat 2>/dev/null | wc -l | tr -d ' ')
  printf "  %-7s %6s files %9s LOC\n" ".$e" "$n" "$loc"
done

echo; echo "== Build / manifests =="
find . -maxdepth 3 \( -name CMakeLists.txt -o -name Makefile -o -name "*.csproj" -o -name "*.sln" -o -name package.json \
  -o -name pyproject.toml -o -name requirements.txt -o -name build.gradle -o -name Package.swift -o -name Cargo.toml \
  -o -name meson.build -o -name "*.pro" -o -name ProjectSettings -o -name project.godot \) $ex 2>/dev/null | head -30

echo; echo "== Engine / framework / graphics API hints =="
grep -rIlE --include='*' -e 'UnityEngine|using Unity' . 2>/dev/null | head -1 | sed 's/^/  Unity: /'
[ -f project.godot ] && echo "  Godot project"
for pat in 'SDL_|SDL2' 'GLFW|glfw' 'glad|GL/gl|OpenGL|glBind' 'vulkan|Vk[A-Z]' 'd3d11|d3d12|DirectX|ID3D' 'Metal|MTL[A-Z]' \
           'SFML' 'raylib' 'Qt[A-Z]|QApplication' 'imgui|ImGui' 'box2d|b2World' 'bullet|btRigid' 'OpenAL|FMOD|miniaudio' \
           'pygame' 'three\.js|THREE\.' 'WebGL|getContext\(.webgl' 'Electron|electron' 'tkinter|PyQt|wx\.'; do
  c=$(grep -rIlE --exclude-dir={node_modules,.git,target,build} -e "$pat" . 2>/dev/null | wc -l | tr -d ' ')
  [ "$c" -gt 0 ] && printf "  %-40s %s files\n" "$pat" "$c"
done

echo; echo "== Assets =="
for e in png jpg jpeg tga bmp dds ktx wav ogg mp3 flac ttf otf obj fbx gltf glb json xml yaml toml ini dat bin sav; do
  n=$(find . -type f -name "*.$e" $ex 2>/dev/null | wc -l | tr -d ' ')
  [ "$n" -gt 0 ] && printf "  .%-5s %s\n" "$e" "$n"
done

echo; echo "== Tests / CI =="
find . -type d \( -name test -o -name tests -o -name __tests__ -o -name spec \) $ex 2>/dev/null | head -5
ls .github/workflows 2>/dev/null

echo; echo "== Global state / unsafe-ish hotspots (port risk) =="
printf "  global/static mutable : %s\n" "$(grep -rIE --exclude-dir={node_modules,.git} '^\s*(static|extern)\s+[^(]*=|\bglobal\b' . 2>/dev/null | wc -l | tr -d ' ')"
printf "  raw pointers / malloc : %s\n" "$(grep -rIE --include='*.c' --include='*.cpp' --include='*.h' --include='*.hpp' '\bmalloc\(|\bnew\s+\w+\[|reinterpret_cast|\(void\s*\*\)' . 2>/dev/null | wc -l | tr -d ' ')"
printf "  threads               : %s\n" "$(grep -rIE 'std::thread|pthread_|Thread\(|threading\.|Task\.Run|async ' . 2>/dev/null | wc -l | tr -d ' ')"
printf "  RNG use               : %s\n" "$(grep -rIE '\brand\(|srand|Random\(|random\.|Math\.random' . 2>/dev/null | wc -l | tr -d ' ')"
