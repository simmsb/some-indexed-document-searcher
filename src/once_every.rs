pub struct OnceEvery {
    count: u64,
    every: u64,
}

impl OnceEvery {
    pub fn new(every: u64) -> Self {
        OnceEvery { count: 0, every }
    }
}

impl Iterator for OnceEvery {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        if self.count == self.every {
            self.count = 0;
            Some(true)
        } else {
            self.count += 1;
            Some(false)
        }
    }
}
