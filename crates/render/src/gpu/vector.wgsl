// Retained local geometry -> GPU instance preparation -> ordered tile bitsets ->
// analytic coverage. No CPU-expanded triangle payload or geometry readback.
struct Node { bounds: vec4<f32>, links: vec4<u32> }
struct Instance {
    matrix: vec4<f32>, low: vec4<f32>, translation: vec4<f32>, color: vec4<f32>,
    source: vec4<u32>, screen: vec4<u32>
}
struct Job { instance: u32, start: u32 }
struct Params { size: vec4<u32>, counts: vec4<u32> }
@group(0) @binding(0) var<storage,read> source_triangles: array<Triangle>;
@group(0) @binding(1) var<storage,read> source_nodes: array<Node>;
@group(0) @binding(2) var<storage,read> instances: array<Instance>;
@group(0) @binding(3) var<storage,read> jobs: array<Job>;
@group(0) @binding(4) var<storage,read_write> triangles: array<Triangle>;
@group(0) @binding(5) var<storage,read_write> nodes: array<Node>;
@group(0) @binding(6) var<storage,read_write> tiles: array<u32>;
@group(0) @binding(7) var<uniform> params: Params;
@group(0) @binding(8) var output: texture_storage_2d<rgba32float,write>;
@group(0) @binding(9) var<storage,read_write> invalid: atomic<u32>;

// Error-compensated products/sums: the input vertices are f32 but authored
// matrices are f64. Retain low coefficient parts instead of silently rounding
// the transform to f32 before evaluating it.
fn coordinate(x:f32,y:f32,a:f32,b:f32,t:f32,al:f32,bl:f32,tl:f32)->f32 {
    let p=a*x; let q=b*y;
    let s=p+q; let v=s-p;
    let error=(p-(s-v))+(q-v)+fma(a,x,-p)+fma(b,y,-q)+al*x+bl*y;
    let r=s+t; let w=r-s;
    return r+((s-(r-w))+(t-w)+error+tl);
}
fn transform(p:vec2<f32>,i:Instance)->vec2<f32> {
    return vec2<f32>(
        coordinate(p.x,p.y,i.matrix.x,i.matrix.z,i.translation.x,i.low.x,i.low.z,i.translation.z),
        coordinate(p.x,p.y,i.matrix.y,i.matrix.w,i.translation.y,i.low.y,i.low.w,i.translation.w));
}
@compute @workgroup_size(64)
fn prepare(@builtin(workgroup_id) group:vec3<u32>,@builtin(local_invocation_index) lane:u32) {
    let job_index=group.x+group.y*65535u;
    if job_index>=params.counts.y {return;}
    let job=jobs[job_index]; let instance=instances[job.instance]; let index=job.start+lane;
    if index<instance.source.z {
        let t=source_triangles[instance.source.x+index];
        triangles[instance.screen.x+index]=Triangle(transform(t.a,instance),transform(t.b,instance),transform(t.c,instance),vec2<f32>(0.0),t.weights);
    } else if index<instance.source.z+instance.source.w {
        let index=index-instance.source.z;
        let n=source_nodes[instance.source.y+index];
        let a=transform(n.bounds.xy,instance); let b=transform(n.bounds.zy,instance);
        let c=transform(n.bounds.xw,instance); let d=transform(n.bounds.zw,instance);
        let bounds=vec4<f32>(min(min(a,b),min(c,d)),max(max(a,b),max(c,d)));
        // Conservative rounding allowance for hierarchy/tile rejection only.
        let pad=max(max(abs(bounds.x),abs(bounds.y)),max(abs(bounds.z),abs(bounds.w)))*0.000001+0.001;
        nodes[instance.screen.y+index]=Node(bounds+vec4<f32>(-pad,-pad,pad,pad),
            vec4<u32>(instance.screen.x+n.links.x,n.links.y,instance.screen.y+n.links.z,0u));
    }
}
@compute @workgroup_size(64)
fn bin(@builtin(global_invocation_id) id:vec3<u32>) {
    let tile=id.y+id.z*65535u;
    let word=id.x;
    if word>=params.size.w || tile>=params.size.z*((params.size.y+15u)/16u) {return;}
    let origin=vec2<f32>(f32(tile%params.size.z)*16.0,f32(tile/params.size.z)*16.0);
    var bits=0u;
    for(var bit=0u;bit<32u;bit++) {
        let index=word*32u+bit;
        if index>=params.counts.x {break;}
        let bounds=nodes[instances[index].screen.y].bounds;
        if all(bounds.xy<origin+vec2<f32>(16.0)) && all(bounds.zw>origin) {bits|=1u<<bit;}
    }
    tiles[tile*params.size.w+word]=bits;
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3<u32>) {
    if any(id.xy>=params.size.xy) {return;}
    let tile=(id.y/16u)*params.size.z+id.x/16u;
    let pixel=vec2<f32>(id.xy);
    var color=vec4<f32>(0.0);
    // Traverse front to back. Premultiplied contributions are weighted by
    // remaining transparency; only exactly opaque coverage terminates early.
    var remaining=1.0;
    for(var reverse_word=params.size.w;reverse_word>0u && remaining>0.0;reverse_word--) {
        let word=reverse_word-1u;
        var bits=tiles[tile*params.size.w+word];
        while bits!=0u && remaining>0.0 {
            let bit=firstLeadingBit(bits); bits&=~(1u<<bit);
            let instance=instances[word*32u+bit];
            var at=instance.screen.y;
            let end=at+instance.source.w;
            var coverage=0.0;
            var visits=0u;
            while at<end && visits<instance.source.w {
                visits++;
                let node=nodes[at];
                if node.links.y>8u || node.links.z<=at || node.links.z>end {atomicStore(&invalid,2u);break;}
                if any(node.bounds.xy>=pixel+vec2<f32>(1.0)) || any(node.bounds.zw<=pixel) {
                    at=node.links.z; continue;
                }
                for(var t=node.links.x;t<node.links.x+node.links.y;t++) {coverage+=area(triangles[t],pixel);}
                at++;
            }
            if at<end {atomicStore(&invalid,2u);}
            let alpha=clamp(coverage,0.0,1.0)*instance.color.a;
            let weight=alpha*remaining;
            color+=vec4<f32>(instance.color.rgb*weight,weight);
            remaining*=1.0-alpha;
        }
    }
    color=vec4<f32>(color.rgb,1.0-remaining);
    if any(color!=color) || any(abs(color)>vec4<f32>(3.402823e38)) {atomicStore(&invalid,1u);}
    textureStore(output,vec2<i32>(id.xy),color);
}
