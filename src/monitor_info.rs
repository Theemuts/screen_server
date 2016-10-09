use std::process::Command;
use regex::Regex;
use std::cmp::Ordering;

#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub name: String,
    pub width: u32,
    pub height: u32,

    pub offset_x: i32,
    pub offset_y: i32,

    pub view_width: u32,
    pub view_height: u32,

    pub raw_bpp: isize,

    pub midpoints_x: Vec<u32>,
    pub midpoints_y: Vec<u32>,
}

impl MonitorInfo {
    pub fn new(name: String, width: u32, height: u32, offset_x: i32, offset_y: i32) -> Self {
        let view_width = 640;
        let view_height = 368;

        /* --------------------------------------------------------------- */
        /* --------------------------------------------------------------- */

        let n_midpoints_x = if width % view_width == 0 {
            width / view_width
        } else {
            width / view_width + 1
        };

        let mut midpoints_x = Vec::with_capacity(n_midpoints_x as usize);
        midpoints_x.push(320);

        for i in 1..n_midpoints_x - 1 {
            midpoints_x.push(320 + i * (width - 640) / (n_midpoints_x - 1));
        }

        midpoints_x.push(width - 320);

        /* --------------------------------------------------------------- */
        /* --------------------------------------------------------------- */

        let n_midpoints_y = if height% view_height== 0 {
            height/ view_height
        } else {
            height/ view_height+ 1
        };

        let mut midpoints_y = Vec::with_capacity(n_midpoints_y as usize);
        midpoints_y.push(184);

        for i in 1..n_midpoints_y - 1 {
            midpoints_y.push(184 + i * (height- 368) / (n_midpoints_y - 1));
        }

        midpoints_y.push(height- 184);

        MonitorInfo {
            name: name,
            width: width,
            height: height,

            offset_x: offset_x,
            offset_y: offset_y,

            view_width: view_width,
            view_height: view_height,

            raw_bpp: 4,

            midpoints_x: midpoints_x,
            midpoints_y: midpoints_y,
        }
    }

    pub fn get_all() -> Vec<MonitorInfo> {
        let str = String::from_utf8(Command::new("xrandr").output().unwrap().stdout).unwrap();
        let re = Regex::new(r"(.+) connected.* (\d+)x(\d+)\+(\d+)\+(\d+)").unwrap();

        let mut name;
        let mut width;
        let mut height;
        let mut offset_x;
        let mut offset_y;

        let mut res = Vec::with_capacity(4);

        for caps in re.captures_iter(&str) {

            name = caps.at(1)
                       .unwrap()
                       .to_string();

            width = caps.at(2)
                        .unwrap()
                        .parse()
                        .unwrap();

            height = caps.at(3)
                         .unwrap()
                         .parse()
                         .unwrap();

            offset_x= caps.at(4)
                          .unwrap()
                          .parse()
                          .unwrap();

            offset_y= caps.at(5)
                          .unwrap()
                          .parse()
                          .unwrap();

            res.push(MonitorInfo::new(name,
                                      width,
                                      height,
                                      offset_x,
                                      offset_y));
        }

        res.sort_by(|a, b| {
            match a.offset_x.cmp(&b.offset_x) {
                Ordering::Equal => {
                    a.offset_y.cmp(&b.offset_y)
                }
                other => other
            }
        });

        println!("{:?}", res);

        res
    }

    pub fn serialize_vec(data: &Vec<Self>) -> Vec<u8> {
        let mut serialized: Vec<u8> = Vec::new();

        for _ in 0..8 {
            serialized.push(255);
        }

        serialized.push(data.len() as u8);

        for mon in data {
            serialized.extend(mon.serialize().into_iter());
        }

        serialized
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut serialized: Vec<u8> = Vec::new();

        let name: Vec<u8> = self.name.clone().into();
        serialized.push(name.len() as u8);
        serialized.extend(name.into_iter());
        serialized.push(self.midpoints_x.len() as u8);
        serialized.push(self.midpoints_y.len() as u8);

        serialized
    }

}