//! The peers a computer has met, and how each is reached (`R`: a WebRTC peer, a UDP address, through the host).
//! A token is whoever first said it on a route: a hello can neither move a known token to another route (and take
//! over that peer: its messages, its seat, the host's word) nor add a second token on a route, nor make a second
//! host. No socket here: the link says what arrived and asks who is who.

use crate::Token;

#[derive(Clone, Debug, PartialEq)]
pub struct Peer<R> {
    pub token: Token,
    pub route: R,
    pub name: String,
    pub locals: u8,
    pub host: bool,
}

/// Why a hello was not taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejected {
    /// That token is another route's.
    TokenTaken,
    /// Not a token anyone draws (they are odd: 0 stands for this computer's own link).
    BadToken,
    /// That route already said another token.
    RouteTaken,
    /// Another peer is the host.
    SecondHost,
}

pub struct Peers<R> {
    list: Vec<Peer<R>>,
}

impl<R> Default for Peers<R> {
    fn default() -> Self {
        Self { list: Vec::new() }
    }
}

impl<R: Copy + PartialEq> Peers<R> {
    /// A hello on `route`: Ok(true) a new peer, Ok(false) one known (its nickname and players may change, not
    /// whether it hosts).
    pub fn meet(&mut self, route: R, token: Token, name: String, locals: u8, host: bool) -> Result<bool, Rejected> {
        if token.is_multiple_of(2) {
            return Err(Rejected::BadToken);
        }
        if let Some(p) = self.list.iter_mut().find(|p| p.token == token) {
            if p.route != route {
                return Err(Rejected::TokenTaken);
            }
            (p.name, p.locals) = (name, locals);
            return Ok(false);
        }
        if self.list.iter().any(|p| p.route == route) {
            return Err(Rejected::RouteTaken);
        }
        if host && self.list.iter().any(|p| p.host) {
            return Err(Rejected::SecondHost);
        }
        self.list.push(Peer { token, route, name, locals, host });
        Ok(true)
    }

    /// The room's other guests, reached through the host (a UDP guest learns them from the host's list, not from
    /// hellos): added, and those no longer listed dropped.
    pub fn via(&mut self, route: R, members: &[(Token, String, u8)], me: Token) {
        for (token, name, locals) in members {
            if *token != me && token % 2 == 1 && !self.list.iter().any(|p| p.token == *token) {
                self.list.push(Peer { token: *token, route, name: name.clone(), locals: *locals, host: false });
            }
        }
        self.list.retain(|p| p.route != route || members.iter().any(|m| m.0 == p.token));
    }

    pub fn host(&self) -> Option<Token> {
        self.list.iter().find(|p| p.host).map(|p| p.token)
    }

    pub fn token_of(&self, route: R) -> Option<Token> {
        self.list.iter().find(|p| p.route == route).map(|p| p.token)
    }

    pub fn route_of(&self, token: Token) -> Option<R> {
        self.list.iter().find(|p| p.token == token).map(|p| p.route)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Peer<R>> {
        self.list.iter()
    }

    /// Drop the peer of a token; its route.
    pub fn remove_token(&mut self, token: Token) -> Option<R> {
        let i = self.list.iter().position(|p| p.token == token)?;
        Some(self.list.remove(i).route)
    }

    /// Drop the peer of a route; its token.
    pub fn remove_route(&mut self, route: R) -> Option<Token> {
        let i = self.list.iter().position(|p| p.route == route)?;
        Some(self.list.remove(i).token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_belongs_to_the_route_that_said_it_first() {
        let mut p: Peers<u8> = Peers::default();
        assert_eq!(p.meet(1, 101, "HOST".into(), 1, true), Ok(true));
        assert_eq!(p.meet(2, 201, "A".into(), 2, false), Ok(true));
        // the host's token from another route: refused, the host stays reached where it was
        assert_eq!(p.meet(2, 101, "X".into(), 1, true), Err(Rejected::TokenTaken));
        assert_eq!(p.meet(3, 101, "X".into(), 1, true), Err(Rejected::TokenTaken));
        assert_eq!(p.route_of(101), Some(1));
        // a second token on a route, a second host, a token nobody draws: refused
        assert_eq!(p.meet(2, 301, "B".into(), 1, false), Err(Rejected::RouteTaken));
        assert_eq!(p.meet(3, 401, "C".into(), 1, true), Err(Rejected::SecondHost));
        assert_eq!(p.meet(4, 0, "Z".into(), 1, false), Err(Rejected::BadToken));
        assert_eq!(p.meet(4, 8, "Z".into(), 1, false), Err(Rejected::BadToken));
        // said again: known; it cannot make itself the host
        assert_eq!(p.meet(2, 201, "A2".into(), 1, true), Ok(false));
        assert_eq!(p.host(), Some(101));
        assert_eq!(p.iter().find(|x| x.token == 201).map(|x| (x.name.as_str(), x.locals)), Some(("A2", 1)));
        // gone, then back on a new route: a new peer
        assert_eq!(p.remove_route(2), Some(201));
        assert_eq!(p.meet(5, 201, "A".into(), 1, false), Ok(true));
    }

    #[test]
    fn guests_through_the_host_follow_its_list() {
        let mut p: Peers<u8> = Peers::default();
        p.meet(1, 101, "HOST".into(), 1, true).unwrap();
        p.via(9, &[(101, "HOST".into(), 1), (7, "ME".into(), 1), (201, "A".into(), 1), (301, "B".into(), 2)], 7);
        assert_eq!(p.iter().count(), 3);
        assert_eq!(p.route_of(301), Some(9));
        p.via(9, &[(101, "HOST".into(), 1), (7, "ME".into(), 1), (201, "A".into(), 1)], 7);
        assert_eq!(p.route_of(301), None);
        assert_eq!(p.route_of(101), Some(1), "the host stays direct");
    }
}
