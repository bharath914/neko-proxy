use flow::{flags, FlowRecord};
use std::collections::{HashMap,VecDeque};
#[derive(Default)]pub struct Audit { recent:VecDeque<Seen>, keys:HashMap<[u8;32],(u64,i64)>, routes:HashMap<[u8;32],HashMap<[u8;32],(u64,i64)>> }
struct Seen {id:u64,key:[u8;32],route:[u8;32]}
fn digest(parts:&[&[u8]])->[u8;32]{let mut hash=blake3::Hasher::new();for p in parts{hash.update(&(p.len() as u64).to_le_bytes());hash.update(p);}*hash.finalize().as_bytes()}
impl Audit {
    pub fn inspect(&mut self,r:&FlowRecord)->u32 {
        let mut bits=decode::static_flags(r);
        let authority=format!("{}://{}:{}",r.scheme,r.host,r.port);
        let key=digest(&[r.method.as_bytes(),authority.as_bytes(),r.path.as_bytes(),r.query.as_bytes(),&r.req_body[..r.req_body.len().min(65536)]]);
        let route_path=decode::route_path(&r.path);
        let route=digest(&[r.method.as_bytes(),authority.as_bytes(),route_path.as_bytes()]);
        if self.keys.get(&key).is_some_and(|(id,time)|*id!=r.id&&r.started_at_ms.abs_diff(*time)<=2000){bits|=flags::DUPLICATE;}
        let group=self.routes.entry(route).or_default();
        group.insert(key,(r.id,r.started_at_ms));
        if r.method=="GET"&&group.values().filter(|(_,time)|r.started_at_ms.abs_diff(*time)<=2000).take(5).count()>=5{bits|=flags::N_PLUS_ONE;}
        self.keys.insert(key,(r.id,r.started_at_ms));
        self.recent.push_back(Seen{id:r.id,key,route});
        if self.recent.len()>4096{let old=self.recent.pop_front().unwrap();
            if self.keys.get(&old.key).is_some_and(|(id,_)|*id==old.id){self.keys.remove(&old.key);}
            if let Some(group)=self.routes.get_mut(&old.route){
                if group.get(&old.key).is_some_and(|(id,_)|*id==old.id){group.remove(&old.key);}
                if group.is_empty(){self.routes.remove(&old.route);}
            }
        }
        bits
    }
}
#[cfg(test)]mod tests{use super::*;#[test]fn duplicates_and_n_plus_one(){let mut a=Audit::default();let mut r=FlowRecord{id:1,method:"GET".into(),host:"api.test".into(),path:"/users/1".into(),..Default::default()};assert_eq!(a.inspect(&r)&flags::DUPLICATE,0);r.id=2;assert_ne!(a.inspect(&r)&flags::DUPLICATE,0);for i in 2..=5{r.id=i+1;r.path=format!("/users/{i}");let bits=a.inspect(&r);if i==5{assert_ne!(bits&flags::N_PLUS_ONE,0);}}r.id=20;r.started_at_ms=5000;assert_eq!(a.inspect(&r)&flags::N_PLUS_ONE,0);}}
